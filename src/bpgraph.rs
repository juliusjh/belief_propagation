#[cfg(feature = "progress_output")]
use std::io::{self, Write};

use std::collections::HashMap;
use std::default::Default;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::{BPError, BPResult, Msg, Node, NodeFunction, Probability};

pub type NodeIndex = usize;

pub struct BPGraph<T, MsgT: Msg<T>, CtrlMsgT = (), CtrlMsgAT: Default = ()>
where
    T: Debug,
{
    nodes: Vec<Node<T, MsgT, CtrlMsgT, CtrlMsgAT>>,
    step: usize,
    normalize: bool,
    check_validity: bool,
}

impl<T, MsgT: Msg<T>, CtrlMsgT, CtrlMsgAT: Default> BPGraph<T, MsgT, CtrlMsgT, CtrlMsgAT>
where
    T: Debug,
    MsgT: Clone,
{
    pub fn initialize_node_constant_msg(
        &mut self,
        node_index: NodeIndex,
        msg: MsgT,
    ) -> BPResult<()> {
        let n = self.get_node_mut(node_index)?;
        for m_i in n.get_connections().clone() {
            n.send_post(m_i, msg.clone());
        }
        n.initialize()?;
        Ok(())
    }
}

impl<T, MsgT: Msg<T> + Clone, CtrlMsgT, CtrlMsgAT: Default> BPGraph<T, MsgT, CtrlMsgT, CtrlMsgAT>
where
    T: Copy + Eq + Debug + std::hash::Hash,
    MsgT: Clone,
{
    pub fn get_result(
        &self,
        node_index: NodeIndex,
    ) -> BPResult<Option<std::collections::HashMap<T, Probability>>> {
        let n = self.get_node(node_index)?;
        n.get_result().map_err(|e| {
            e.attach_info_str(
                "BPGraph::get_result",
                format!("Failed to retrieve result from node {}", node_index),
            )
        })
    }
}

impl<T, MsgT: Msg<T>, CtrlMsgT, CtrlMsgAT: Default> BPGraph<T, MsgT, CtrlMsgT, CtrlMsgAT>
where
    T: Send + Sync + Debug,
    MsgT: Send + Sync,
{
    //msgs: [(from, [(to, msg)])]
    fn send_threaded(
        &mut self,
        msgs: Vec<(NodeIndex, Vec<(NodeIndex, MsgT)>)>,
        thread_count: u32,
    ) -> BPResult<()> {
        let normalize = self.normalize;
        let check_validity = self.check_validity;
        let step = self.step;
        let mut nodes: Vec<Arc<Mutex<&mut Node<T, MsgT, CtrlMsgT, CtrlMsgAT>>>> = self
            .nodes
            .iter_mut()
            .map(|n| Arc::new(Mutex::new(n)))
            .collect();
        let ln_msgs = msgs.len();
        let mut msgs = Arc::new(Mutex::new(msgs));
        let min_batch_size = 5;
        #[cfg(feature = "progress_output")]
        let (whitespace_padding, step) = {
            let max_diff_in_number = f64::log10(ln_msgs as f64) as usize + 1;
            (
                &(std::iter::repeat(" ")
                    .take(max_diff_in_number)
                    .collect::<String>()),
                self.step.clone(),
            )
        };
        crossbeam::scope(|scope| {
            let mut handles = Vec::with_capacity(thread_count as usize);
            for i in 0..thread_count {
                handles.push(scope.spawn(|_| {
                    loop {
                        let chunck: Vec<(NodeIndex, Vec<(NodeIndex, MsgT)>)> = {
                            thread_print!("Thread {} waiting for lock..", i);
                            let mut msgs = msgs.lock().expect("Locking mutex failed.");
                            thread_print!("Thread {} has lock..", i);
                            let len = msgs.len();
                            if len == 0 {
                                break;
                            }

                            #[cfg(feature = "progress_output")]
                            {
                                print!(
                                    "Step {}: {} nodes left{}\r",
                                    step,
                                    nodes.len(),
                                    &whitespace_padding
                                );
                                std::io::stdout().flush();
                            }
                            let mut batch_size = std::cmp::max(
                                min_batch_size,
                                msgs.len() / (2 * thread_count) as usize,
                            ); //TODO
                            let chunck = msgs
                                .drain(0..std::cmp::min(batch_size as usize, len))
                                .collect();
                            chunck
                        };

                        for (from, mut msgmap) in chunck.into_iter() {
                            for (to, mut msg) in msgmap.into_iter() {
                                debug_print!("Sending from {} to {}", from, to);
                                {
                                    if check_validity && !msg.is_valid() {
                                        return Err(BPError::new(
                                            "BPGraph::send".to_owned(),
                                            format!("Trying to send an invalid message ({} -> {})", from, to),
                                        )
                                        .attach_debug_object("msg (the invalid message)", &msg)
                                        .attach_debug_object("step", step));
                                    }
                                    if normalize {
                                        msg.normalize().map_err(|e| {
                                            e.attach_info_str(
                                                "BPGraph::send",
                                                format!("Trying to normalize message {} -> {}.", from, to),
                                            )
                                            .attach_debug_object("msg (the message that could not be normalized)", &msg)
                                            .attach_debug_object("step", step)
                                        })?;
                                    }
                                }
                                let mut nto = nodes[to].lock().expect("Locking node failed");
                                if !nto.get_connections().contains(&from) {
                                    return Err(BPError::new(
                                        "BPGraph::send".to_owned(),
                                        format!(
                                            "Trying to send a message along a none existing edge ({} -> {}).",
                                            from, to
                                        ),
                                    )
                                    .attach_debug_object("step", step));
                                }
                                nto.send_post(from, msg);
                            }
                        }
                    }
                    Ok(())
                }));
            }
            for handle in handles {
                handle.join().expect("Joining threads failed")?;
            }
            #[cfg(feature = "progress_output")]
            {
                let whitespace_padding2 = std::iter::repeat(" ").take(30).collect::<String>(); //Not very elegant...
                print!("{}{}\r", whitespace_padding2, &whitespace_padding);
                std::io::stdout().flush();
            }
            Ok(())
        }).expect("Scoped threading failed")
    }

    fn create_messages_threaded(
        &mut self,
        thread_count: u32,
    ) -> BPResult<Vec<(NodeIndex, Vec<(NodeIndex, MsgT)>)>> {
        info_print!("Creating messages with {} threads..", thread_count);
        let step = self.step;
        let mut nodes_: Vec<(NodeIndex, &mut Node<T, MsgT, CtrlMsgT, CtrlMsgAT>)> = self
            .nodes
            .iter_mut()
            .enumerate()
            .filter(|(_, n)| n.is_ready(step).expect("Is ready failed"))
            .collect();
        let mut min_batch_size = 5;
        #[cfg(feature = "progress_output")]
        let (whitespace_padding, step) = {
            let max_diff_in_number = f64::log10(nodes_.len() as f64) as usize + 1;
            (
                &(std::iter::repeat(" ")
                    .take(max_diff_in_number)
                    .collect::<String>()),
                self.step.clone(),
            )
        };
        thread_print!("Minimal batch size is {}", min_batch_size);
        let mut nodes = Arc::new(Mutex::new(nodes_));

        crossbeam::scope(|scope| {
            let mut handles = Vec::with_capacity(thread_count as usize);
            let mut result = Vec::new();
            for i in 0..thread_count {
                //Force capture by ref
                let nodes = &nodes;
                handles.push(scope.spawn(move |_| {
                    let mut thread_msgs = Vec::new();
                    loop {
                        //nodes is locked in this block
                        let chunck: Vec<(NodeIndex, &mut Node<T, MsgT, CtrlMsgT, CtrlMsgAT>)> = {
                            thread_print!("Thread {} waiting for lock..", i);
                            let mut nodes = nodes.lock().expect("Locking mutex failed.");
                            thread_print!("Thread {} has lock..", i);
                            let len = nodes.len();
                            if len == 0 {
                                break;
                            }

                            #[cfg(feature = "progress_output")]
                            {
                                print!(
                                    "Step {}: {} nodes left{}\r",
                                    step,
                                    nodes.len(),
                                    &whitespace_padding
                                );
                                std::io::stdout().flush();
                            }
                            let mut batch_size = std::cmp::max(
                                min_batch_size,
                                nodes.len() / (2 * thread_count) as usize,
                            ); //TODO
                            let chunck = nodes
                                .drain(0..std::cmp::min(batch_size as usize, len))
                                .collect();
                            chunck
                        };
                        thread_print!("Thread {} working on {} nodes..", i, chunck.len());
                        if chunck.is_empty() {
                            break;
                        }
                        for (idx, node) in chunck {
                            thread_msgs.push((
                                idx,
                                node.create_messages().map_err(|e| {
                                    e.attach_debug_object("idx (node index)", idx)
                                        .attach_debug_object(
                                            "node.get_name() (node name)",
                                            node.get_name(),
                                        )
                                        .attach_debug_object("step", step)
                                })?,
                            ));
                        }
                    }
                    thread_print!("Thread {} finished.", i);
                    Ok(thread_msgs)
                }));
            }
            for handle in handles {
                result.extend(handle.join().expect("Joining threads failed")?);
            }
            #[cfg(feature = "progress_output")]
            {
                let whitespace_padding2 = std::iter::repeat(" ").take(30).collect::<String>(); //Not very elegant...
                print!("{}{}\r", whitespace_padding2, &whitespace_padding);
                std::io::stdout().flush();
            }
            Ok(result)
        })
        .expect("Scoped threading failed.")
    }

    pub fn propagate_step_threaded(&mut self, thread_count: u32) -> BPResult<()> {
        if self.check_validity && !self.is_valid() {
            return Err(BPError::new(
                "propagate_step_threaded".to_owned(),
                "Graph is invalid".to_owned(),
            ));
        }
        info_print!("Propagating step {}..", self.step);
        debug_print!("Creating messages..");
        let outgoing_msgs = self.create_messages_threaded(thread_count)?;
        info_print!("Sending messages");
        self.send_threaded(outgoing_msgs, thread_count)?;
        info_print!("Done propagating step {}\n", self.step);
        self.step += 1;
        Ok(())
    }

    pub fn propagate_threaded(&mut self, steps: usize, thread_count: u32) -> BPResult<()> {
        if !self.is_initialized() {
            return Err(BPError::new(
                "propagate_threaded".to_owned(),
                "Graph is not initialized".to_owned(),
            ));
        }
        for _ in 0..steps {
            self.propagate_step_threaded(thread_count)?;
        }
        Ok(())
    }
    pub fn factor_nodes_count(&self) -> usize {
        self.nodes.iter().filter(|&n| n.is_factor()).count()
    }
    pub fn variable_nodes_count(&self) -> usize {
        self.nodes.iter().filter(|&n| !n.is_factor()).count()
    }
    pub fn nodes_count(&self) -> usize {
        self.nodes.len()
    }
}

impl<T, MsgT: Msg<T>, CtrlMsgT, CtrlMsgAT: Default> BPGraph<T, MsgT, CtrlMsgT, CtrlMsgAT>
where
    T: Clone + Debug,
    MsgT: Clone,
{
    pub fn get_inbox(&self, node_index: NodeIndex) -> BPResult<Vec<(NodeIndex, MsgT)>> {
        let node = self.get_node(node_index)?;
        Ok(node.clone_inbox())
    }
}

impl<T, MsgT: Msg<T>, CtrlMsgT, CtrlMsgAT: Default> BPGraph<T, MsgT, CtrlMsgT, CtrlMsgAT>
where
    T: Debug,
{
    pub fn new() -> Self {
        BPGraph {
            nodes: Vec::new(),
            step: 0,
            normalize: true,
            check_validity: false,
        }
    }

    pub fn set_normalize(&mut self, normalize: bool) {
        self.normalize = normalize;
    }

    pub fn send_control_message(
        &mut self,
        node_index: NodeIndex,
        ctrl_msg: CtrlMsgT,
    ) -> BPResult<CtrlMsgAT> {
        self.get_node_mut(node_index)?
            .send_control_message(ctrl_msg)
    }

    pub fn set_check_validity(&mut self, value: bool) {
        self.check_validity = value;
    }

    pub fn is_initialized(&self) -> bool {
        self.nodes.iter().all(|n| n.is_initialized())
    }

    pub fn reset(&mut self) -> BPResult<()> {
        self.nodes.iter_mut().try_for_each(|n| n.reset())
    }

    pub fn initialize_node(
        &mut self,
        node_index: NodeIndex,
        msgs: Option<Vec<(NodeIndex, MsgT)>>,
    ) -> BPResult<()> {
        if msgs.is_some() {
            self.send(vec![(node_index, msgs.unwrap())]);
        }
        let node = self.get_node_mut(node_index)?;
        node.initialize()?;
        Ok(())
    }

    pub fn initialize(&mut self) -> BPResult<()> {
        self.nodes.iter_mut().try_for_each(|node| {
            if !node.is_initialized() {
                node.initialize()
            } else {
                Ok(())
            }
        })
    }

    pub fn propagate(&mut self, steps: usize) -> BPResult<()> {
        if !self.is_initialized() {
            return Err(BPError::new(
                "BPGraph::propagate".to_owned(),
                "Graph is not initialized".to_owned(),
            ));
        }
        for _ in 0..steps {
            self.propagate_step()?;
        }
        Ok(())
    }

    pub fn propagate_step(&mut self) -> BPResult<()> {
        if self.check_validity && !self.is_valid() {
            return Err(BPError::new(
                "BPGraph::propagate_step".to_owned(),
                "Invalid graph".to_owned(),
            ));
        }
        info_print!("Propagating step {}", self.step);
        info_print!("Creating messages");
        let outgoing_msgs = self.create_messages()?;
        info_print!("Sending messages");
        self.send(outgoing_msgs)?;
        info_print!("Done propagating step {}\n", self.step);
        self.step += 1;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    //Returns Node (from) -> (Node(to) -> Msg)
    fn create_messages(&mut self) -> BPResult<Vec<(NodeIndex, Vec<(NodeIndex, MsgT)>)>> {
        let mut res = Vec::new();
        for (i, node) in self.nodes.iter_mut().enumerate() {
            if node.is_ready(self.step)? {
                debug_print!("Creating messages at node <{}>", node.get_name());
                res.push((
                    i,
                    node.create_messages().map_err(|e| {
                        e.attach_debug_object("i", i)
                            .attach_debug_object("node.get_name()", node.get_name())
                    })?,
                ));
            }
        }
        Ok(res)
    }

    //msgs: [(from, [(to, msg)])]
    fn send(&mut self, msgs: Vec<(NodeIndex, Vec<(NodeIndex, MsgT)>)>) -> BPResult<()> {
        let normalize = self.normalize;
        let check_validity = self.check_validity;
        let step = self.step;
        for (from, mut msgmap) in msgs.into_iter() {
            for (to, mut msg) in msgmap.into_iter() {
                debug_print!("Sending from {} to {}", from, to);
                let nto = self.get_node_mut(to)?;
                if !nto.get_connections().contains(&from) {
                    return Err(BPError::new(
                        "BPGraph::send".to_owned(),
                        format!(
                            "Trying to send a message along a none existing edge ({} -> {}).",
                            from, to
                        ),
                    )
                    .attach_debug_object("step", step));
                }
                if normalize {
                    msg.normalize().map_err(|e| {
                        e.attach_info_str(
                            "BPGraph::send",
                            format!("Trying to normalize message {} -> {}.", from, to),
                        )
                        .attach_debug_object("msg (the message that could not be normalized)", &msg)
                        .attach_debug_object("step", step)
                    })?;
                }
                if check_validity && !msg.is_valid() {
                    return Err(BPError::new(
                        "BPGraph::send".to_owned(),
                        format!("Trying to send an invalid message ({} -> {})", from, to),
                    )
                    .attach_debug_object("msg (the invalid message)", &msg)
                    .attach_debug_object("step", step));
                }
                nto.send_post(from, msg);
            }
        }
        Ok(())
    }

    pub fn reserve(&mut self, number_nodes: usize) {
        self.nodes.reserve(number_nodes);
    }

    pub fn add_node(
        &mut self,
        name: String,
        node_function: Box<dyn NodeFunction<T, MsgT, CtrlMsgT, CtrlMsgAT> + Send + Sync>,
    ) -> NodeIndex {
        self.nodes.push(Node::<T, MsgT, CtrlMsgT, CtrlMsgAT>::new(
            name,
            node_function,
        ));
        self.nodes.len() - 1
    }

    pub fn add_edge(&mut self, node0: NodeIndex, node1: NodeIndex) -> BPResult<()> {
        debug_print!("Connecting nodes {} and {}", node0, node1);
        if self.get_node(node0)?.is_factor() == self.get_node(node1)?.is_factor() {
            debug_print!("Cannot link nodes: {} and {}", node0, node1);
            return Err(BPError::new(
                "BPGraph::add_edge".to_owned(),
                format!(
                    "Cannot link two nodes of same type (variable/factor) ({}, {})",
                    node0, node1
                ),
            ));
        }
        {
            let n0 = self.get_node_mut(node0)?;
            n0.add_edge(node1)?;
        }
        let n1 = self.get_node_mut(node1)?;
        n1.add_edge(node0)?;
        Ok(())
    }

    fn get_node(&self, node: NodeIndex) -> BPResult<&Node<T, MsgT, CtrlMsgT, CtrlMsgAT>> {
        let len = self.len();
        self.nodes.get(node).ok_or(BPError::new(
            "BPGraph::get_node".to_owned(),
            format!("Index {} out of bounds ({})", node, len),
        ))
    }
    fn get_node_mut(
        &mut self,
        node: NodeIndex,
    ) -> BPResult<&mut Node<T, MsgT, CtrlMsgT, CtrlMsgAT>> {
        let len = self.len();
        self.nodes.get_mut(node).ok_or(BPError::new(
            "BPGraph::get_node".to_owned(),
            format!("Index {} out of bounds ({})", node, len),
        ))
    }
    pub fn is_valid(&self) -> bool {
        debug_print!("Checking graph");
        self.nodes
            .iter()
            .enumerate()
            .all(|(i, _)| self.is_valid_node(i))
    }
    pub fn is_valid_node(&self, node: NodeIndex) -> bool {
        let nres = self.get_node(node);
        if nres.is_err() {
            info_print!("Could not find node {}", node);
            return false;
        }
        let n = nres.ok().unwrap();
        let cons = n.get_connections();
        if cons.is_empty() {
            info_print!("Node {} has no edges", node);
            return false;
        }
        let number_inputs = n.number_inputs();
        if number_inputs.is_some() && number_inputs.unwrap() != cons.len() {
            info_print!(
                "Node {} has a wrong number ({}) of inputs (should be: {})",
                node,
                cons.len(),
                number_inputs.unwrap()
            );
            return false;
        }
        for con in n.get_connections() {
            let nconres = self.get_node(*con);
            if nconres.is_err() {
                info_print!("Could not find node {} in connections of {}", con, node);
                return false;
            }
            let ncon = nconres.ok().unwrap();
            if !ncon.get_connections().contains(&node) {
                info_print!(
                    "{} does not have {} as connection but {} has {} as connection",
                    ncon,
                    node,
                    node,
                    ncon
                );
                return false;
            }
        }
        true
    }
}

impl<T, MsgT: Msg<T>, CtrlMsgT, CtrlMsgAT: Default> std::fmt::Display
    for BPGraph<T, MsgT, CtrlMsgT, CtrlMsgAT>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for (i, node) in self.nodes.iter().enumerate() {
            writeln!(f, "{}:\t{}", i, node);
        }
        writeln!(f)
    }
}
