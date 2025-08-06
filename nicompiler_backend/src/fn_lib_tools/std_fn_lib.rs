//! The library of built-in waveform functions

use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;
use std::f64::consts::PI;
use fn_lib_macros::{std_fn_f64, std_fn_bool};
use crate::fn_lib_tools::{Calc, FnBoxF64, FnBoxBool};

#[pyclass]
pub struct StdFnLib {}

#[pymethods]
impl StdFnLib {
    #[new]
    pub fn new() -> Self {
        Self {}
    }
}

// region F64 functions
/// Constant function:
///     val: value
#[std_fn_f64]
pub struct ConstF64 {
    val: f64
}
impl Calc<f64> for ConstF64 {
    fn calc(&self, _t_arr: &[f64], res_arr: &mut [f64]) {
        res_arr.fill(self.val)
    }
}

/// Linear function:
/// `LinFn(t) = slope*t + offs`
#[std_fn_f64]
pub struct LinFn {
    slope: f64,
    offs: f64,
}
impl Calc<f64> for LinFn {
    fn calc(&self, t_arr: &[f64], res_arr: &mut[f64]) {
        for (res, &t) in res_arr.iter_mut().zip(t_arr.iter()) {
            *res = self.slope * t + self.offs
        }
    }
}

/// Sine function:
///     amp - amplitude (in Volts)
///     freq - linear frequency (in Hz)
///     phase - absolute phase (in radians)
///     offs - offset (in Volts)
/// `Sine(t) = amp * sin(2Pi * freq * t + phase) + offs`
#[std_fn_f64(amp, freq, phase=0.0, offs=0.0)]
pub struct Sine {
    amp: f64,
    freq: f64,
    phase: f64,
    offs: f64,
}
impl Calc<f64> for Sine {
    fn calc(&self, t_arr: &[f64], res_arr: &mut[f64]) {
        for (res, &t) in res_arr.iter_mut().zip(t_arr.iter()) {
            *res = self.offs + self.amp * f64::sin(2.0*PI * self.freq * t + self.phase)
        }
    }
}

/// Gaussian function:
/// `Gaussian(t) = scale * exp[-(t - t0)^2 / (2 * sigma^2)] + offs`
#[std_fn_f64(t0, sigma, scale, offs=0.0)]
pub struct Gaussian {
    t0: f64,
    sigma: f64,
    scale: f64,
    offs: f64,
}
impl Calc<f64> for Gaussian {
    fn calc(&self, t_arr: &[f64], res_arr: &mut [f64]) {
        let denominator = 2.0 * self.sigma.powi(2);
        for (res, &t) in res_arr.iter_mut().zip(t_arr.iter()) {
            *res = self.offs + self.scale * f64::exp(
                -(t - self.t0).powi(2) / denominator
            )
        }
    }
}

/// Lorentzian function:
/// `Lorentzian(t) = scale / ((t-t0)/tau)^2 + 1) + offs`
#[std_fn_f64(t0, tau, scale, offs=0.0)]
pub struct Lorentzian {
    t0: f64,
    tau: f64,
    scale: f64,
    offs: f64,
}
impl Calc<f64> for Lorentzian {
    fn calc(&self, t_arr: &[f64], res_arr: &mut [f64]) {
        for (res, &t) in res_arr.iter_mut().zip(t_arr.iter()) {
            *res = self.offs + self.scale / (
                ((t - self.t0) / self.tau).powi(2) + 1.0
            )
        }
    }
}

/// Hyperbolic tangent function:
/// `TanH(t) = scale * tanh[(t - t0)/tau] + offs`
#[std_fn_f64(t0, tau, scale, offs=0.0)]
pub struct TanH {
    t0: f64,
    tau: f64,
    scale: f64,
    offs: f64,
}
impl Calc<f64> for TanH {
    fn calc(&self, t_arr: &[f64], res_arr: &mut [f64]) {
        for (res, &t) in res_arr.iter_mut().zip(t_arr.iter()) {
            *res = self.offs + self.scale * f64::tanh((t - self.t0) / self.tau)
        }
    }
}

/// Exponential function:
/// `Exp(t) = scale * exp(t/tau) + offs`
#[std_fn_f64(tau, scale, offs=0.0)]
pub struct Exp {
    tau: f64,
    scale: f64,
    offs: f64
}
impl Calc<f64> for Exp {
    fn calc(&self, t_arr: &[f64], res_arr: &mut [f64]) {
        for (res, &t) in res_arr.iter_mut().zip(t_arr.iter()) {
            *res = self.offs + self.scale * f64::exp(t / self.tau)
        }
    }
}

#[derive(Clone, Debug)]
pub struct Poly {
    prms: Vec<f64>,
}
impl Poly {
    pub fn new(prms: Vec<f64>) -> Self {
        Self { prms }
    }
}
#[pymethods]
impl StdFnLib {
    #[allow(non_snake_case)]
    /// Polynomial function with the `prms` vector of coefficients:
    /// `Poly(t; prms) = prms[0] + prms[1]*t + prms[2]*t^2 + ... + prms[n-1]*t^(n-1)`
    fn Poly(&self, prms: Vec<f64>) -> PyResult<FnBoxF64> {
        if prms.is_empty() {
            Err(PyValueError::new_err("Empty coefficient vector passed"))
        } else {
            let fn_inst = Poly::new(prms);
            let fn_box = FnBoxF64 { inner: Box::new(fn_inst) };
            Ok(fn_box)
        }
    }
}
impl Calc<f64> for Poly {
    fn calc(&self, t_arr: &[f64], res_arr: &mut [f64]) {
        for (prm_idx, &prm_val) in self.prms.iter().enumerate() {
            if prm_idx == 0 {
                for res in res_arr.iter_mut() {
                    *res = prm_val
                }
            } else {
                for (res, &t) in res_arr.iter_mut().zip(t_arr.iter()) {
                    *res += prm_val * f64::powi(t,prm_idx as i32)
                }
            }
        }
    }
}

/// Power function:
/// `Pow(t) = scale*(t - t0)^pow + offs`
/// In contrast to `Poly`, this function only includes a single term + offset
/// but allows for an arbitrary real-valued power
#[std_fn_f64(t0, pow, scale, offs=0.0)]
pub struct Pow {
    t0: f64,
    pow: f64,
    scale: f64,
    offs: f64,
}
impl Calc<f64> for Pow {
    fn calc(&self, t_arr: &[f64], res_arr: &mut [f64]) {
        for (res, &t) in res_arr.iter_mut().zip(t_arr.iter()) {
            *res = self.offs + self.scale * (t - self.t0).powf(self.pow)
        }
    }
}
// endregion

// region Bool functions
/// Boolean constant:
///     val - value
#[std_fn_bool]
pub struct ConstBool {
    val: bool
}
impl Calc<bool> for ConstBool {
    fn calc(&self, _t_arr: &[f64], res_arr: &mut [bool]) {
        res_arr.fill(self.val)
    }
}
// endregion
