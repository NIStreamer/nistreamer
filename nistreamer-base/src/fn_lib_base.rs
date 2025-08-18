use pyo3::prelude::*;
use std::fmt::Debug;

pub trait Calc<T> {
    fn calc(&self, t_arr: &[f64], res_arr: &mut [T]);
}

pub trait FnTraitSet<T>: Calc<T> + Debug + Send + Sync {
    fn clone_to_box(&self) -> Box<dyn FnTraitSet<T>>;
}

impl<S, T> FnTraitSet<T> for S
    where S: Calc<T> + Clone + Debug + Send + Sync + 'static
{
    fn clone_to_box(&self) -> Box<dyn FnTraitSet<T>> {
        Box::new(self.clone())
    }
}

impl<T> Clone for Box<dyn FnTraitSet<T>> {
    fn clone(&self) -> Self {
        self.clone_to_box()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct FnBoxF64 {
    pub inner: Box<dyn FnTraitSet<f64>>
}

#[pyclass]
#[derive(Clone)]
pub struct FnBoxBool {
    pub inner: Box<dyn FnTraitSet<bool>>
}