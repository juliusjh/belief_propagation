use crate::{BPResult, Msg, NodeIndex, Probability};
use std::default::Default;
use std::fmt::Debug;

pub trait NodeFunction<T, MsgT: Msg<T>, CtrlMsgT = (), CtrlMsgAT: Default = ()> {
    fn node_function(&mut self, inbox: Vec<(NodeIndex, MsgT)>) -> BPResult<Vec<(NodeIndex, MsgT)>>;
    fn is_factor(&self) -> bool;
    fn number_inputs(&self) -> Option<usize>;
    fn initialize(&mut self, connections: Vec<NodeIndex>) -> BPResult<()>;
    fn is_ready(&self, recv_from: &Vec<(NodeIndex, MsgT)>, current_step: usize) -> BPResult<bool>;
    fn reset(&mut self) -> BPResult<()>;
    fn get_prior(&self) -> Option<MsgT>;
    fn send_control_message(&mut self, ctrl_msg: CtrlMsgT) -> BPResult<CtrlMsgAT> {
        Ok(CtrlMsgAT::default())
    }
    fn discard_mode(&self) -> bool {
        false
    }
}
