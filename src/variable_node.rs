use crate::{BPError, BPResult, Msg, NodeFunction, NodeIndex, Probability};
use std::cmp::Eq;
use std::fmt::Debug;
use std::hash::Hash;

#[derive(Clone)]
pub enum InputNeed {
    AlwaysExceptFirst,
    Always,
    NeverExceptFirst,
    Never,
}

#[derive(Clone)]
pub struct VariableNode<T, MsgT: Msg<T>> {
    //TODO:
    is_log: bool,
    connections: Option<Vec<NodeIndex>>,
    prior: Option<MsgT>,
    is_threaded: bool,
    needs_all_inputs: InputNeed,
    has_propagated: bool,
    send_to_all: bool,
    phantom: std::marker::PhantomData<T>,
}

impl<T, MsgT: Msg<T>> VariableNode<T, MsgT>
where
    MsgT: Clone
{
    #[allow(dead_code)]
    pub fn new() -> Self {
        VariableNode {
            is_log: false,
            connections: None,
            prior: None,
            is_threaded: true,
            needs_all_inputs: InputNeed::AlwaysExceptFirst,
            has_propagated: false,
            send_to_all: false,
            phantom: std::marker::PhantomData,
        }
    }
    pub fn set_prior(&mut self, prior: &MsgT) -> BPResult<()> {
        if self.prior.is_some() {
            return Err(BPError::new(
                "VariableNode::set_prior".to_owned(),
                "Prior is already set".to_owned(),
            ));
        }
        self.prior = Some(prior.clone());
        Ok(())
    }

    pub fn set_input_need(&mut self, input_need: InputNeed) {
        self.needs_all_inputs = input_need;
    }

    pub fn set_threaded(&mut self, is_threaded: bool) {
        self.is_threaded = is_threaded;
    }

    pub fn set_send_to_all(&mut self, send_to_all: bool) {
        self.send_to_all = send_to_all;
    }
}

impl<T, MsgT: Msg<T>> NodeFunction<T, MsgT> for VariableNode<T, MsgT>
where
    MsgT: Clone,
{
    fn is_ready(&self, recv_from: &Vec<(NodeIndex, MsgT)>, step: usize) -> BPResult<bool> {
        Ok(
            if recv_from.len()
                == self
                    .connections
                    .as_ref()
                    .expect("Node not initialized.")
                    .len()
            {
                true
            } else if recv_from.is_empty() && self.prior.is_none() {
                false
            } else {
                match self.needs_all_inputs {
                    InputNeed::AlwaysExceptFirst => !self.has_propagated,
                    InputNeed::NeverExceptFirst => self.has_propagated,
                    InputNeed::Never => true,
                    InputNeed::Always => false,
                }
            },
        )
    }

    fn get_prior(&self) -> Option<MsgT> {
        self.prior.clone()
    }

    fn initialize(&mut self, connections: Vec<NodeIndex>) -> BPResult<()> {
        self.connections = Some(connections);
        Ok(())
    }

    fn node_function(
        &mut self,
        mut inbox: Vec<(NodeIndex, MsgT)>,
    ) -> BPResult<Vec<(NodeIndex, MsgT)>> {
        let connections = self
            .connections
            .as_ref()
            .expect("VariableNode not initialized");
        self.has_propagated = true;
        if inbox.is_empty() {
            if let Some(prior) = &self.prior {
                Ok(connections
                    .iter()
                    .map(|idx| (*idx, prior.clone()))
                    .collect())
            } else {
                Err(BPError::new(
                    "VariableNode::node_function".to_owned(),
                    "Inbox is empty".to_owned(),
                ))
            }
        }
        else if inbox.len() == 1 {
            let (idx_in, mut msg_in) = inbox.pop().unwrap();
            let mut out: Vec<(NodeIndex, MsgT)> = Vec::new();
            if let Some(prior) = &self.prior {
                msg_in.mult_msg(prior);
                out.push((idx_in, prior.clone()));
            }
            for con in connections {
                if idx_in != *con {
                    out.push((*con, msg_in.clone()));
                }
            }
            Ok(out)
        }
        else if inbox.len() == connections.len() || !self.send_to_all {
            let mut result: Vec<(NodeIndex, MsgT)> = Vec::with_capacity(connections.len());
            let n = inbox.len();
            let (mut acc, start) =
            if let Some(prior) = &self.prior {
                (prior.clone(), 0)
            }
            else {
                result.push((inbox[0].0, inbox[0].1.clone()));
                (inbox[0].1.clone(), 1)
            };
            for msg in &inbox[start..] {
                result.push((msg.0, acc.clone()));
                acc.mult_msg(&msg.1);
            }
            acc = inbox[n - 1].1.clone();
            for msg in result[..n-1].iter_mut().rev() {
                msg.1.mult_msg(&acc);
                acc.mult_msg(&msg.1);
            }
            Ok(result)
        }
        else {
            let mut result: Vec<(NodeIndex, MsgT)> = Vec::with_capacity(connections.len());
            let mut missing = connections.clone();
            let n = inbox.len();
            let (mut acc, start) =
            if let Some(prior) = &self.prior {
                (prior.clone(), 0)
            }
            else {
                result.push((inbox[0].0, inbox[0].1.clone()));
                missing.retain(|idx| *idx != inbox[0].0);
                (inbox[0].1.clone(), 1)
            };
            for msg in &inbox[start..] {
                result.push((msg.0, acc.clone()));
                acc.mult_msg(&msg.1);
                missing.retain(|idx| *idx != msg.0);
            }
            acc = inbox[n - 1].1.clone();
            for msg in result[..n-1].iter_mut().rev() {
                msg.1.mult_msg(&acc);
                acc.mult_msg(&msg.1);
            }
            assert_eq!(missing.len() + result.len(), connections.len());
            for idx in missing {
                result.push((idx, acc.clone()));
            }
            Ok(result)

        }
    }

    fn reset(&mut self) -> BPResult<()> {
        self.prior = None;
        Ok(())
    }

    fn is_factor(&self) -> bool {
        false
    }

    fn number_inputs(&self) -> Option<usize> {
        None
    }
}

impl<T, MsgT: Msg<T> + Clone> Default for VariableNode<T, MsgT> {
    fn default() -> Self {
        Self::new()
    }
}