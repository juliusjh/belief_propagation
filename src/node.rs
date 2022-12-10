use crate::{BPError, BPResult, Msg, NodeFunction, NodeIndex, Probability};
use std::collections::HashMap;
use std::default::Default;
use std::fmt::Debug;

pub struct Node<T, MsgT: Msg<T>, CtrlMsgT, CtrlMsgAT: Default>
where
    T: Debug,
{
    name: String,
    connections: Vec<NodeIndex>,
    inbox: Vec<(NodeIndex, MsgT)>,
    node_function: Box<dyn NodeFunction<T, MsgT, CtrlMsgT, CtrlMsgAT> + Send + Sync>,
    is_initialized: bool,
}

impl<T, MsgT: Msg<T>, CtrlMsgT, CtrlMsgAT: Default> Node<T, MsgT, CtrlMsgT, CtrlMsgAT>
where
    T: Debug,
{
    pub fn new(
        name: String,
        node_function: Box<dyn NodeFunction<T, MsgT, CtrlMsgT, CtrlMsgAT> + Send + Sync>,
    ) -> Self {
        let mut inbox = Vec::new();
        let num_input = node_function.number_inputs();
        if let Some(num_input) = num_input {
            inbox.reserve(num_input);
        }
        Node {
            name,
            is_initialized: false,
            connections: Vec::new(),
            inbox,
            node_function,
        }
    }
    pub fn send_control_message(&mut self, ctrl_msg: CtrlMsgT) -> BPResult<CtrlMsgAT> {
        self.node_function.send_control_message(ctrl_msg)
    }
    pub fn get_name(&self) -> &String {
        &self.name
    }
    pub fn add_edge(&mut self, to: NodeIndex) -> BPResult<()> {
        if self.connections.contains(&to) {
            return Err(BPError::new(
                "Node::add_edge".to_owned(),
                format!("Connection -> {} already exists", to),
            ));
        }
        if let Some(n) = self.node_function.number_inputs() {
            if self.connections.len() >= n {
                return Err(BPError::new("Node::add_edge".to_owned(), format!("Wrong number ({}) of connections (needed: {}) while trying to add edge to {}", self.connections.len()+1, n, to)));
            }
        }
        self.connections.push(to);
        Ok(())
    }
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }
    pub fn reset(&mut self) -> BPResult<()> {
        self.node_function.reset()?;
        self.inbox = Vec::new();
        let num_input = self.node_function.number_inputs();
        if let Some(num_input) = num_input {
            self.inbox.reserve(num_input);
        }
        self.is_initialized = false;
        Ok(())
    }
    pub fn number_inputs(&self) -> Option<usize> {
        self.node_function.number_inputs()
    }
    pub fn initialize(&mut self) -> BPResult<()> {
        if self.is_initialized {
            return Err(BPError::new(
                "Node::initialize".to_owned(),
                format!("Node {} is already initialized", self.name),
            ));
        }
        if let Some(n) = self.node_function.number_inputs() {
            if self.connections.len() != n {
                return Err(BPError::new(
                    "Node::initialize".to_owned(),
                    format!(
                        "Node {} has wrong number ({}) of connections (needs: {})",
                        self.name,
                        self.connections.len(),
                        n
                    ),
                ));
            }
        }
        self.is_initialized = true;
        self.node_function.initialize(self.connections.clone())
    }
    pub fn get_connections(&self) -> &Vec<NodeIndex> {
        &self.connections
    }

    pub fn get_connections_mut(&mut self) -> &mut Vec<NodeIndex> {
        &mut self.connections
    }
    pub fn is_factor(&self) -> bool {
        self.node_function.is_factor()
    }
    pub fn has_post(&self) -> bool {
        !self.inbox.is_empty()
    }

    pub fn read_post(&mut self) -> Vec<(NodeIndex, MsgT)> {
        std::mem::replace(&mut self.inbox, Vec::with_capacity(self.connections.len()))
    }

    pub fn send_post(&mut self, from: NodeIndex, msg: MsgT) {
        self.inbox.push((from, msg));
    }

    pub fn is_ready(&self, step: usize) -> BPResult<bool> {
        self.node_function.is_ready(&self.inbox, step)
    }
    pub fn discard_mode(&self) -> bool {
        self.node_function.discard_mode()
    }
    pub fn create_messages(&mut self) -> BPResult<Vec<(NodeIndex, MsgT)>> {
        let incoming_msgs = self.read_post();
        debug_print!(
            "<{}> starting to create messages: Collected {} incoming messages",
            self.name,
            incoming_msgs.len()
        );
        //TODO: Check in debug mode if all messages arrived?
        self.node_function.node_function(incoming_msgs)
    }
}

impl<T, MsgT: Msg<T>, CtrlMsgT, CtrlMsgAT: Default> Node<T, MsgT, CtrlMsgT, CtrlMsgAT>
where
    T: Clone + Debug,
    MsgT: Clone,
{
    pub fn clone_inbox(&self) -> Vec<(NodeIndex, MsgT)> {
        self.inbox.clone()
    }
}

impl<T, MsgT: Msg<T>, CtrlMsgT, CtrlMsgAT: Default> Node<T, MsgT, CtrlMsgT, CtrlMsgAT>
where
    T: Copy + Eq + std::hash::Hash + Debug,
    MsgT: Clone,
{
    pub fn get_result(&self) -> BPResult<Option<std::collections::HashMap<T, Probability>>> {
        let prior = self.node_function.get_prior();
        if self.inbox.is_empty() {
            // TODO: use everything
            return if let Some(prior) = prior {
                let mut prior_hm = msg_to_hashmap(prior);
                norm_hashmap(&mut prior_hm);
                Ok(Some(prior_hm))
            } else {
                info_print!("Get result: No messages and no prior at node - propagate one step?");
                Ok(None)
            };
        }
        if self.is_factor() {
            info_print!("Results at factor nodes are not implemented yet");
            Ok(None)
        } else {
            let (mut res, start) = if let Some(prior) = self.node_function.get_prior() {
                let mut prior = msg_to_hashmap(prior);
                norm_hashmap(&mut prior);
                (prior, 0)
            } else {
                (msg_to_hashmap(self.inbox[0].1.clone()), 1)
            };
            for inb in &self.inbox[start..] {
                mult_hashmaps(&mut res, msg_to_hashmap(inb.1.clone())).map_err(|e| {
                    e.attach_info_str(
                        "node::get_result",
                        format!(
                            "Failed multiplying hashmaps to compute result for node {}.",
                            self.name
                        ),
                    )
                    .attach_debug_object("inb (element of inbox)", inb)
                    .attach_debug_object("res (Accumulating variable for multiplication, starting with prior or first message)", &res)
                })?;
            }
            //res = self.inbox.iter().fold_result(res, |a, b| mult_hashmaps(a, msg_to_hashmap(b.1.clone()))?);
            Ok(Some(res))
        }
    }
}

pub fn hashmap_to_distribution<T>(map: &mut HashMap<T, Probability>) -> BPResult<()> {
    let sum = map.iter().map(|(_, p)| p).sum::<f64>();
    map.iter_mut().for_each(|(_, p)| *p /= sum);
    Ok(())
}

pub fn norm_hashmap<T>(map: &mut HashMap<T, Probability>) -> BPResult<()>
where
    T: Eq + std::hash::Hash + Debug,
{
    let max: f64 = map
        .iter()
        .map(|(_, p)| p)
        .max_by(|p0, p1| {
            p0.abs()
                .partial_cmp(&p1.abs())
                .unwrap_or(std::cmp::Ordering::Less)
        })
        .map(|p| p.abs())
        .unwrap_or(f64::NAN);
    if max.is_nan() || max == 0.0 {
        return Err(BPError::new(
            "node::norm_hashmap".to_owned(),
            "Could not normalize.".to_owned(),
        )
        .attach_debug_object("map", map));
    }
    map.iter_mut().for_each(|(_, p)| *p /= max);
    Ok(())
}

fn msg_to_hashmap<T, MsgT: Msg<T>>(msg: MsgT) -> HashMap<T, Probability>
where
    T: Eq + std::hash::Hash + Debug,
{
    msg.into_iter().collect()
}

pub fn mult_hashmaps<T>(
    op0: &mut HashMap<T, Probability>,
    op1: HashMap<T, Probability>,
) -> BPResult<()>
where
    T: Copy + Eq + std::hash::Hash + Debug,
{
    for (v, p0) in op1 {
        if let Some(p) = op0.get_mut(&v) {
            *p *= p0;
        }
    }
    norm_hashmap(op0)?;
    Ok(())
}

impl<T, MsgT: Msg<T>, CtrlMsgT, CtrlMsgAT: Default> std::fmt::Display
    for Node<T, MsgT, CtrlMsgT, CtrlMsgAT>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}, {:?}", self.name, self.connections)
    }
}
