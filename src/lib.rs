#![allow(unused)]
#[macro_use]
pub mod macros;
pub mod bperror;
pub mod bpgraph;
pub mod msg;
pub mod node;
pub mod node_function;
pub mod types;
pub mod variable_node;

pub use bperror::{BPError, BPResult};
pub use bpgraph::{BPGraph, NodeIndex};
pub use msg::Msg;
pub use node::hashmap_to_distribution;
pub use node::Node;
pub use node_function::NodeFunction;
pub use types::Probability;
pub use variable_node::VariableNode;

//TODO: Add tests
#[cfg(test)]
mod tests {
    use crate::{
        node_function, BPError, BPGraph, BPResult, Msg, NodeFunction, NodeIndex, Probability,
        VariableNode,
    };
    use std::collections::HashMap;
    use std::fmt::Debug;

    fn mul(x: i32, y: i32) -> Probability {
        if 2 * x == y {
            1.0
        } else {
            0.0
        }
    }

    #[test]
    fn test() -> BPResult<()> {
        let mut g = BPGraph::<i32, HashMap<i32, Probability>>::new();
        let mut v0 = VariableNode::new();
        let mut v1 = VariableNode::new();
        let mut v2 = VariableNode::new();
        let mut dist0 = HashMap::new();
        let mut dist1 = HashMap::new();
        dist0.insert(1, 1.0);
        dist1.insert(1, 0.25);
        dist1.insert(2, 0.25);
        dist1.insert(3, 0.25);
        dist1.insert(4, 0.25);
        v0.set_prior(&dist0);
        v1.set_prior(&dist1);
        v2.set_prior(&dist1);

        let t3 = TwoNode::new(mul);
        let t4 = TwoNode::new(mul);
        g.add_node("0".to_string(), Box::new(v0));
        g.add_node("1".to_string(), Box::new(v1));
        g.add_node("2".to_string(), Box::new(v2));
        g.add_node("m3".to_string(), Box::new(t3));
        g.add_node("m4".to_string(), Box::new(t4));

        g.add_edge(0, 3)?;
        g.add_edge(3, 1)?;
        g.add_edge(1, 4)?;
        g.add_edge(4, 2)?;

        assert!(g.is_valid());

        g.initialize()?;

        g.propagate_threaded(2, 1)?;
        g.propagate(10)?;
        let mut res = g.get_result(2)?.unwrap();
        let mut sum: f64 = res.iter().map(|(_, p)| p).sum();
        let res_normed: HashMap<i32, Probability> =
            res.iter().map(|(v, p)| (*v, p / sum)).collect();

        println!("{:?}", res_normed);
        for (v, p) in res_normed {
            if v == 4 {
                //TODO: Allow a small error
                assert_eq!(p, 1.0);
            } else {
                assert_eq!(p, 0.0);
            }
        }

        Ok(())
    }

    struct TwoNode<T: Debug, MsgT: Msg<T>> {
        f_node_function: fn(T, T) -> Probability,
        connection0: Option<NodeIndex>,
        connection1: Option<NodeIndex>,
        phantom: std::marker::PhantomData<MsgT>,
    }

    impl<T: Debug + Copy, MsgT: Msg<T> + Clone> TwoNode<T, MsgT> {
        pub fn new(f_node_function: fn(T, T) -> Probability) -> Self {
            Self {
                connection0: None,
                connection1: None,
                f_node_function: f_node_function,
                phantom: std::marker::PhantomData,
            }
        }
    }

    impl<T: Debug + Copy + std::fmt::Display, MsgT: Msg<T> + Clone> NodeFunction<T, MsgT>
        for TwoNode<T, MsgT>
    {
        fn node_function(
            &mut self,
            inbox: Vec<(NodeIndex, MsgT)>,
        ) -> BPResult<Vec<(NodeIndex, MsgT)>> {
            if self.connection0.is_none() || self.connection1.is_none() {
                return Err(BPError::new(
                    "TwoNode::node_function".to_owned(),
                    "TwoNode not initialized".to_owned(),
                ));
            }
            if inbox.len() != 2 {
                return Err(BPError::new(
                    "TwoNode::node_function".to_owned(),
                    "Not enough messages".to_owned(),
                ));
            }
            if inbox[0].0 != self.connection0.unwrap() || inbox[1].0 != self.connection1.unwrap() {
                return Err(BPError::new(
                    "TwoNode::node_function".to_owned(),
                    "Received wrong messages".to_owned(),
                ));
            }
            let mut msgout0 = MsgT::new();
            let mut msgout1 = MsgT::new();
            for (val0, p0) in inbox[0].1.clone().into_iter() {
                for (val1, p1) in inbox[1].1.clone().into_iter() {
                    let pf = (self.f_node_function)(val0, val1);
                    //debug_print!("{} {}, {} {}, {}", val0, p0, val1, p1, pf);
                    match msgout0.get_mut(val0) {
                        None => {
                            msgout0.insert(val0, p1 * pf);
                        }
                        Some(pold) => {
                            *pold += p1 * pf;
                        }
                    };
                    match msgout1.get_mut(val1) {
                        None => {
                            msgout1.insert(val1, p0 * pf);
                        }
                        Some(pold) => {
                            *pold += p0 * pf;
                        }
                    };
                }
            }
            for (val, p) in msgout0.clone() {
                //debug_print!("{}: {}", val, p);
            }
            Ok(vec![
                (self.connection0.unwrap(), msgout0),
                (self.connection1.unwrap(), msgout1),
            ])
        }
        fn is_factor(&self) -> bool {
            true
        }
        fn number_inputs(&self) -> Option<usize> {
            Some(2)
        }
        fn initialize(&mut self, connections: Vec<NodeIndex>) -> BPResult<()> {
            if connections.len() != 2 {
                return Err(BPError::new(
                    "TwoNode::initialize".to_owned(),
                    "Two node needs exactly two connections".to_owned(),
                ));
            }
            self.connection0 = Some(connections[0]);
            self.connection1 = Some(connections[1]);
            Ok(())
        }
        fn is_ready(&self, recv_from: &Vec<(NodeIndex, MsgT)>, _clock: usize) -> BPResult<bool> {
            Ok(recv_from.len() == 2)
        }
        fn reset(&mut self) -> BPResult<()> {
            self.connection0 = None;
            self.connection1 = None;
            Ok(())
        }
        fn get_prior(&self) -> Option<MsgT> {
            None
        }
    }
}
