use crate::{BPError, BPResult, Probability};
use std::collections::HashMap;
use std::fmt::Debug;

//TODO: Relax restrictions?
//Disadvantage: Not always needed
//Advantage: Does it really make sense to have a non iterable message? It could lead to confusing problems?
pub trait Msg<T>: Debug
where
    Self: IntoIterator<Item = (T, Probability)>,
{
    fn new() -> Self;
    fn get(&self, value: T) -> Option<Probability>;
    fn get_mut(&mut self, value: T) -> Option<&mut Probability>;
    fn insert(&mut self, value: T, p: Probability);
    fn normalize(&mut self) -> BPResult<()>;
    fn is_valid(&self) -> bool;
    fn mult_msg(&mut self, other: &Self);
}
/*
impl<MsgT: Msg<T>, T: Clone> MultMsg<T> for MsgT
    where for<'a> &'a MsgT: IntoIterator<Item = (T, Probability)>
{
    fn mult_msg(&mut self, other: &Self) {
        for (v1, p1) in other.into_iter() {
            if let Some(p0) = self.get_mut(v1) {
                *p0 *= p1;
            }
        }
        //Do not normalize?
        self.normalize();
    }
}
*/

pub fn mult_hashmaps<T>(op0: &mut HashMap<T, Probability>, op1: &HashMap<T, Probability>)
where
    T: Eq + std::hash::Hash + Debug,
{
    for (v, p0) in op1 {
        if let Some(p) = op0.get_mut(v) {
            *p *= p0;
        }
    }
    crate::node::norm_hashmap(op0);
}

impl<T> Msg<T> for HashMap<T, Probability>
where
    T: std::hash::Hash + Eq + Debug,
{
    fn new() -> Self {
        HashMap::new()
    }
    fn get(&self, value: T) -> Option<Probability> {
        HashMap::get(self, &value).copied()
    }
    fn get_mut(&mut self, value: T) -> Option<&mut Probability> {
        HashMap::get_mut(self, &value)
    }
    fn insert(&mut self, value: T, p: Probability) {
        self.insert(value, p);
    }
    fn normalize(&mut self) -> BPResult<()> {
        if self.is_empty() {
            return Err(BPError::new(
                "HashMap as Msg::normalize".to_owned(),
                "Message is empty".to_owned(),
            ));
        }
        let len = self.len() as Probability;
        for (_, p) in self.iter_mut() {
            *p *= len;
        }
        Ok(())
    }
    fn is_valid(&self) -> bool {
        self.iter()
            .all(|(_, p)| !p.is_nan() && *p >= 0 as Probability && *p <= 1.0 as Probability)
    }
    fn mult_msg(&mut self, other: &Self) {
        mult_hashmaps(self, other);
    }
}

//TODO: indexmap
