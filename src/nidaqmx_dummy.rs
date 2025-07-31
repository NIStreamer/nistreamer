//! Provides a dummy of the minimal rust wrapper for parts of the NI-DAQmx C library.
//!
//! ## Overview
//!
//! The core of this module is the [`NiTask`] struct which represents an NI-DAQmx task. It encapsulates
//! a handle to an NI-DAQmx task and provides methods that map to various DAQmx C-functions, enabling
//! users to perform operations like creating analog or digital channels, configuring sampling rates,
//! and writing data to channels.
//!
//! Additionally, the module provides utility functions like [`daqmx_call`] and [`reset_ni_device`] to
//! simplify error handling and device interactions.
//!
//! **Refer to implementations of the [`NiTask`] struct to see the wrapped methods and invoked
//! [DAQmx C-functions](https://www.ni.com/docs/en-US/bundle/ni-daqmx-c-api-ref/page/cdaqmx/help_file_title.html)**
//!
//! ## Usage
//!
//! Typical usage involves creating an instance of the `NiTask` struct, configuring it (e.g., setting
//! up channels, setting clock rates), and then invoking operations (e.g., starting the task, writing
//! data). All operations are abstracted through safe Rust methods, ensuring type safety and reducing
//! the likelihood of runtime errors.
//!
//! ## Safety and Error Handling
//!
//! Given that this module interfaces with a C library, many of the calls involve unsafe Rust blocks.
//! To mitigate potential issues, this module provides the `daqmx_call` function that wraps DAQmx
//! C-function calls, checks for errors, and handles them appropriately (e.g., logging and panicking).
//! ***In addition to printing, NI-DAQmx driver errors are saved in `nidaqmx_error.logs` file in the
//! directory of the calling shell.
//!
//! ## Constants and Types
//!
//! To ensure type safety and clarity, the module defines several type aliases (e.g., `CConstStr`,
//! `CUint32`) and constants (e.g., `DAQMX_VAL_RISING`, `DAQMX_VAL_VOLTS`) that map to their C
//! counterparts. These are used throughout the module to ensure that function signatures and calls
//! match their expected types.
//!
//! ## Cleanup and Resource Management
//!
//! The `NiTask` struct implements the `Drop` trait, ensuring that resources (like the DAQmx task handle)
//! are cleaned up properly when an instance goes out of scope. This behavior reduces the chance of
//! resource leaks.
//!
//! ## External Dependencies
//!
//! This module depends on the `libc` crate for C types and the `ndarray` crate for multi-dimensional
//! arrays. It also uses the `std::fs` and `std::io` modules for file operations, specifically for logging
//! errors.
//!
//! ## Example
//!
//! ```ignore
//! # use niexpctrl_backend::*;
//! let task = NiTask::new();
//! task.create_ao_chan("Dev1/ao0");
//! task.cfg_sample_clk("", 1000.0, 1000);
//! // ... other configurations and operations ...
//! task.start();
//! // ... write data, wait, etc. ...
//! task.stop();
//! ```
//!
//! ## Further Reading
//!
//! For more details on the NI-DAQmx C driver and its capabilities, please refer to the
//! [NI-DAQmx C Reference](https://www.ni.com/docs/en-US/bundle/ni-daqmx-c-api-ref/page/cdaqmx/help_file_title.html).

use std::cell::{Cell, RefCell};
use std::fs::File;
use std::io::{Write, Error};
use std::time;
use serde::Serialize;
use indexmap::IndexMap;
use std::ffi::NulError;
use libc;
type CInt32 = libc::c_int;
pub const DAQMX_VAL_STARTTRIGGER: CInt32 = 12491;
pub const DAQMX_VAL_SAMPLECLOCK: CInt32 = 12487;

#[derive(Clone, Debug)]
pub struct DAQmxError {
    msg: String
}
impl DAQmxError {
    pub fn new(msg: String) -> Self {
        Self {msg}
    }
}
impl ToString for DAQmxError {
    fn to_string(&self) -> String {
        self.msg.clone()
    }
}
impl From<NulError> for DAQmxError {
    fn from(value: NulError) -> Self {
        DAQmxError::new(format!("Failed to convert '{}' to CString", value.to_string()))
    }
}
impl From<std::io::Error> for DAQmxError {
    fn from(value: Error) -> Self {
        DAQmxError::new(value.to_string())
    }
}

/// Resets a specified National Instruments (NI) device.
///
/// This function attempts to reset the provided NI device by invoking the `DAQmxResetDevice` method.
///
/// # Parameters
///
/// * `name`: A reference to a string slice representing the name of the NI device to be reset.
///
/// # Behavior
///
/// The function first converts the provided device name to a `CString` to ensure compatibility with the C-function call.
/// It then invokes the `daqmx_call` function to safely call the `DAQmxResetDevice` method.
///
/// # Safety
///
/// This function contains an unsafe block due to the direct interaction with the C library, specifically when calling the `DAQmxResetDevice` method.
///
/// # Example
/// ```ignore
/// # use niexpctrl_backend::*;
/// reset_ni_device("PXI1Slot3");
/// ```
///
/// # Panics
///
/// This function will panic if:
/// * There's a failure in converting the device name to a `CString`.
/// * The `DAQmxResetDevice` call returns a negative error code (handled by `daqmx_call`).
///
/// # Note
///
/// Ensure that the device name provided is valid and that the device is accessible when invoking this function.
pub fn reset_device(name: &str) -> Result<(), DAQmxError> {
    println!("nidaqmx::reset_device(name={name}) called");
    Ok(())
}
pub fn connect_terms(src: &str, dest: &str) -> Result<(), DAQmxError> {
    println!("nidaqmx::connect_terms(src={src}, dest={dest}) called");
    Ok(())
}
pub fn disconnect_terms(src: &str, dest: &str) -> Result<(), DAQmxError> {
    println!("nidaqmx::disconnect_terms(src={src}, dest={dest}) called");
    Ok(())
}

/// Represents a National Instruments (NI) DAQmx task.
///
/// `NiTask` encapsulates a handle to an NI-DAQmx task, providing a Rust-friendly interface to interact with the task.
/// Creating an instance of this struct corresponds to creating a new NI-DAQmx task. Methods on the struct
/// allow for invoking the associated DAQmx methods on the task.
///
/// The struct primarily holds a task handle, represented by the `handle` field, which is used for internal
/// operations and interactions with the DAQmx C API.
///
/// # NI-DAQmx Reference
///
/// For detailed information about the underlying driver and its associated methods, refer to the
/// [NI-DAQmx C Reference](https://www.ni.com/docs/en-US/bundle/ni-daqmx-c-api-ref/page/cdaqmx/help_file_title.html).
///
/// # Examples
///
/// ```ignore
/// let task = NiTask::new();
/// // task.some_method();
/// ```
///
/// # Note
///
/// Ensure you have the necessary NI-DAQmx drivers and libraries installed and accessible when using this struct and its associated methods.
#[derive(Serialize)]
pub struct NiTask {
    samp_rate: Cell<f64>,
    buf_size: Cell<usize>,
    dev_name: RefCell<String>,
    ao_chans: RefCell<IndexMap<String, Vec<f64>>>,  // the `Vec<>` is used to store written samples if `DUMP_SAMPS` is `true`
    do_chans: RefCell<IndexMap<String, Vec<u32>>>,  // the `Vec<>` is used to store written samples if `DUMP_SAMPS` is `true`
    start_trig_in: RefCell<Option<String>>,
    start_trig_out: RefCell<Option<String>>,
    samp_clk_in: RefCell<Option<String>>,
    samp_clk_out: RefCell<Option<String>>,
    ref_clk_in: RefCell<Option<String>>,
    seq_len: RefCell<Option<u64>>,
    last_written_vals_f64: RefCell<Option<Vec<f64>>>,
    last_written_vals_u32: RefCell<Option<Vec<u32>>>,
    start_walltime_nanos: RefCell<Option<u128>>,
}

impl NiTask {
    // File dump switches:
    /*
        If `DUMP_INFO` is `true`, `NiTask::stop()` call will save serialized `NiTask` to `dev_name.json`
        in the current working directory.

        In addition, if `DUMP_SAMPS` is also `true`, samples from all `write_analog/digital_...()` calls
        will be accumulated by `NiTask` and then also included into same dump file.
        CAUTION - this mode will accumulate ALL samples for the entire sequence duration and not just
        the single-write buffer potentially resulting in a huge amount of data held in RAM at once.
        Only use this mode with low sample rate and/or short sequences.
    */
    const DUMP_INFO: bool = true;
    const DUMP_SAMPS: bool = false;

    pub fn new() -> Result<Self, DAQmxError> {
        Ok(Self {
            samp_rate: Cell::new(0.0),
            buf_size: Cell::new(0),
            dev_name: RefCell::new("".to_string()),
            ao_chans: RefCell::new(IndexMap::new()),
            do_chans: RefCell::new(IndexMap::new()),
            start_trig_in: RefCell::new(None),
            start_trig_out: RefCell::new(None),
            samp_clk_in: RefCell::new(None),
            samp_clk_out: RefCell::new(None),
            ref_clk_in: RefCell::new(None),
            seq_len: RefCell::new(None),
            last_written_vals_f64: RefCell::new(None),
            last_written_vals_u32: RefCell::new(None),
            start_walltime_nanos: RefCell::new(None),
        })
    }

    pub fn clear(&self) -> Result<(), DAQmxError> {
        self.samp_rate.set(0.0);
        self.buf_size.set(0);
        self.dev_name.borrow_mut().clear();
        self.ao_chans.borrow_mut().clear();
        self.do_chans.borrow_mut().clear();
        *self.start_trig_in.borrow_mut() = None;
        *self.start_trig_out.borrow_mut() = None;
        *self.samp_clk_in.borrow_mut() = None;
        *self.samp_clk_out.borrow_mut() = None;
        *self.ref_clk_in.borrow_mut() = None;
        *self.seq_len.borrow_mut() = None;
        *self.last_written_vals_f64.borrow_mut() = None;
        *self.last_written_vals_u32.borrow_mut() = None;
        *self.start_walltime_nanos.borrow_mut() = None;
        Ok(())
    }
    pub fn start(&self) -> Result<(), DAQmxError> {
        *self.start_walltime_nanos.borrow_mut() = Some(
            time::SystemTime::now()
            .duration_since(time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
        );
        Ok(())
    }
    pub fn stop(&self) -> Result<(), DAQmxError> {
        *self.start_walltime_nanos.borrow_mut() = None;
        if Self::DUMP_INFO {
            self.dump_to_file()?;
        }
        Ok(())
    }
    pub fn wait_until_done(&self, _timeout: Option<f64>) -> Result<(), DAQmxError> {
        std::thread::sleep(std::time::Duration::from_millis(100));  // Mimic typical ~100-150 ms buffer play time
        Ok(())
    }
    pub fn disallow_regen(&self) -> Result<(), DAQmxError> {
        Ok(())
    }

    pub fn cfg_samp_clk_timing_continuous_samps(&self, clk_src: &str, samp_rate: f64) -> Result<(), DAQmxError> {
        self.samp_rate.set(samp_rate);
        *self.samp_clk_in.borrow_mut() = Some(clk_src.to_string());
        if Self::DUMP_SAMPS {
            self.ao_chans.borrow_mut().values_mut().for_each(|vec| *vec = Vec::new());
            self.do_chans.borrow_mut().values_mut().for_each(|vec| *vec = Vec::new());
        }
        Ok(())
    }

    pub fn cfg_samp_clk_timing_finite_samps(&self, clk_src: &str, samp_rate: f64, samps_per_chan: u64) -> Result<(), DAQmxError> {
        self.samp_rate.set(samp_rate);
        *self.seq_len.borrow_mut() = Some(samps_per_chan);
        *self.samp_clk_in.borrow_mut() = Some(clk_src.to_string());
        if Self::DUMP_SAMPS {
            self.ao_chans.borrow_mut().values_mut().for_each(|vec| *vec = Vec::with_capacity(samps_per_chan as usize));
            self.do_chans.borrow_mut().values_mut().for_each(|vec| *vec = Vec::with_capacity(samps_per_chan as usize));
        }
        Ok(())
    }

    pub fn cfg_output_buf(&self, buf_size: usize) -> Result<(), DAQmxError> {
        self.buf_size.set(buf_size);
        Ok(())
    }

    pub fn create_ao_chan(&self, name: &str) -> Result<(), DAQmxError> {
        if let Some(dev_name) = Self::extract_dev_name(name) {
            *self.dev_name.borrow_mut() = dev_name.to_string();
        }
        self.ao_chans.borrow_mut().insert(name.to_string(), Vec::new());
        Ok(())
    }

    pub fn create_do_port(&self, name: &str) -> Result<(), DAQmxError> {
        if let Some(dev_name) = Self::extract_dev_name(name) {
            *self.dev_name.borrow_mut() = dev_name.to_string();
        }
        self.do_chans.borrow_mut().insert(name.to_string(), Vec::new());
        Ok(())
    }

    pub fn write_digital_port(&self, samp_buf: &[u32], samp_num: usize, _timeout: Option<f64>) -> Result<usize, DAQmxError> {
        if Self::DUMP_SAMPS {
            for (chan_idx, chan_samps) in self.do_chans.borrow_mut().values_mut().enumerate() {
                chan_samps.extend_from_slice(&samp_buf[chan_idx * samp_num .. (chan_idx + 1) * samp_num]);
            }
        }

        // Imitate waiting for buffer space to become available as samples are generated out
        if self.last_written_vals_u32.borrow().is_some() {
            let approx_wait_time = samp_num as f64 / self.samp_rate.get();
            std::thread::sleep(std::time::Duration::from_secs_f64(approx_wait_time));
        }

        // Safe last written values
        let last_written_vals = Self::pick_last_vals(samp_buf, self.do_chans.borrow().len(), samp_num)?;
        *self.last_written_vals_u32.borrow_mut() = Some(last_written_vals);

        Ok(samp_num)
    }

    /*pub fn write_digital_lines(&self, _samp_buf: &[u8], samp_num: usize, _timeout: Option<f64>) -> Result<usize, DAQmxError> {
        let approx_wait_time = 0.8 * 100e-9 * samp_num as f64;  // 100 ns - sample clock period assuming 10 MSa/s sampling rate for DO card
        std::thread::sleep(std::time::Duration::from_secs_f64(approx_wait_time));
        Ok(samp_num)
    }*/

    pub fn write_analog(&self, samp_buf: &[f64], samp_num: usize, _timeout: Option<f64>) -> Result<usize, DAQmxError> {
        if Self::DUMP_SAMPS {
            for (chan_idx, chan_samps) in self.ao_chans.borrow_mut().values_mut().enumerate() {
                chan_samps.extend_from_slice(&samp_buf[chan_idx * samp_num .. (chan_idx + 1) * samp_num]);
            }
        }

        // Imitate waiting for buffer space to become available as samples are generated out
        if self.last_written_vals_f64.borrow().is_some() {
            let approx_wait_time = samp_num as f64 / self.samp_rate.get();
            std::thread::sleep(std::time::Duration::from_secs_f64(approx_wait_time));
        }

        // Safe last written values
        let last_written_vals = Self::pick_last_vals(samp_buf, self.ao_chans.borrow().len(), samp_num)?;
        *self.last_written_vals_f64.borrow_mut() = Some(last_written_vals);

        Ok(samp_num)
    }

    pub fn get_last_written_vals_f64(&self) -> Option<Vec<f64>> {
        self.last_written_vals_f64.borrow().clone()
    }

    pub fn get_last_written_vals_u32(&self) -> Option<Vec<u32>> {
        self.last_written_vals_u32.borrow().clone()
    }

    pub fn set_ref_clk_rate(&self, _rate: f64) -> Result<(), DAQmxError> {
        Ok(())
    }

    pub fn set_ref_clk_src(&self, src: &str) -> Result<(), DAQmxError> {
        *self.ref_clk_in.borrow_mut() = Some(src.to_string());
        Ok(())
    }

    pub fn cfg_ref_clk(&self, src: &str, rate: f64) -> Result<(), DAQmxError> {
        self.set_ref_clk_rate(rate)?;
        self.set_ref_clk_src(src)?;
        Ok(())
    }

    pub fn cfg_dig_edge_start_trigger(&self, trigger_source: &str) -> Result<(), DAQmxError> {
        *self.start_trig_in.borrow_mut() = Some(trigger_source.to_string());
        Ok(())
    }

    pub fn get_write_current_write_pos(&self) -> Result<u64, DAQmxError> {
        Ok(0)
    }

    pub fn export_signal(&self, signal_id: CInt32, output_terminal: &str) -> Result<(), DAQmxError> {
        match signal_id {
            DAQMX_VAL_STARTTRIGGER => *self.start_trig_out.borrow_mut() = Some(output_terminal.to_string()),
            DAQMX_VAL_SAMPLECLOCK => *self.samp_clk_out.borrow_mut() = Some(output_terminal.to_string()),
            _ => println!("NiTask::export_signal() - unknown signal_id: {signal_id}"),
        }
        Ok(())
    }

    pub fn get_write_total_samp_per_chan_generated(&self) -> Result<u64, DAQmxError> {
        if let Some(start_walltime_nanos) = self.start_walltime_nanos.borrow().clone() {
            let current_walltime_nanos = time::SystemTime::now()
                .duration_since(time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let elapsed_run_time_secs = (current_walltime_nanos - start_walltime_nanos) as f64 * 1e-9;
            let elapsed_clk_periods = (self.samp_rate.get() * elapsed_run_time_secs).round() as u64;
            Ok(elapsed_clk_periods)
        } else {
            Err(DAQmxError::new("get_write_total_samp_per_chan_generated(): task was not started".to_string()))
        }
    }

    /// Extracts "dev_name" assuming channel name format "/dev_name/ao0" or "/dev_name/port0"
    fn extract_dev_name(full_chan_name: &str) -> Option<&str> {
        let mut parts = full_chan_name.split('/');
        parts.nth(1)
    }

    fn dump_to_file(&self) -> Result<(), std::io::Error> {
        let serialized = serde_json::to_string(&self)?;

        let file_path = format!("{}.json", self.dev_name.borrow());
        let mut file = File::create(file_path)?;
        file.write_all(serialized.as_bytes())?;

        Ok(())
    }

    /// Utility function. Recommended for use in save `last_written_vals` implementation
    fn pick_last_vals<T: Clone>(samp_buf: &[T], chan_num: usize, samp_num: usize) -> Result<Vec<T>, DAQmxError> {
        // Sanity check:
        if chan_num * samp_num > samp_buf.len() {
            return Err(DAQmxError::new(format!(
                "[pick_last_vals()] expected sample number {} exceeds buffer length {}", chan_num * samp_num, samp_buf.len()
            )))
        }

        let mut last_vals = Vec::with_capacity(chan_num);
        for chan_idx in 0..chan_num {
            let last_samp_idx = (chan_idx + 1) * samp_num - 1;
            let last_val = samp_buf.get(last_samp_idx).unwrap().clone();
            last_vals.push(last_val);
        }

        Ok(last_vals)
    }
}