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

use std::ffi::NulError;
use libc;
type CInt32 = libc::c_int;

pub const DAQMX_VAL_STARTTRIGGER: CInt32 = 12491;
pub const DAQMX_VAL_SAMPLECLOCK: CInt32 = 12487;

#[derive(Clone)]
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
pub fn reset_device(_name: &str) -> Result<(), DAQmxError> {
    Ok(())
}
pub fn connect_terms(_src: &str, _dest: &str) -> Result<(), DAQmxError> {
    Ok(())
}
pub fn disconnect_terms(_src: &str, _dest: &str) -> Result<(), DAQmxError> {
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
pub struct NiTask {}

impl NiTask {
    pub fn new() -> Result<Self, DAQmxError> {
        Ok(Self {})
    }

    pub fn clear(&self) -> Result<(), DAQmxError> {
        Ok(())
    }
    pub fn start(&self) -> Result<(), DAQmxError> {
        Ok(())
    }
    pub fn stop(&self) -> Result<(), DAQmxError> {
        Ok(())
    }
    pub fn wait_until_done(&self, _timeout: Option<f64>) -> Result<(), DAQmxError> {
        std::thread::sleep(std::time::Duration::from_millis(100));  // Mimic typical ~100-150 ms buffer play time
        Ok(())
    }
    pub fn disallow_regen(&self) -> Result<(), DAQmxError> {
        Ok(())
    }

    pub fn cfg_samp_clk_timing(&self, _clk_src: &str, _samp_rate: f64, _seq_len: u64) -> Result<(), DAQmxError> {
        Ok(())
    }

    pub fn cfg_output_buf(&self, _buf_size: usize) -> Result<(), DAQmxError> {
        Ok(())
    }

    pub fn create_ao_chan(&self, _name: &str) -> Result<(), DAQmxError> {
        Ok(())
    }

    pub fn create_do_chan(&self, _name: &str) -> Result<(), DAQmxError> {
        Ok(())
    }

    pub fn write_digital_port(&self, _samp_buf: &[u32], samp_num: usize, _timeout: Option<f64>) -> Result<usize, DAQmxError> {
        let approx_wait_time = 0.8 * 100e-9 * samp_num as f64;  // 100 ns - sample clock period assuming 10 MSa/s sampling rate for DO card
        std::thread::sleep(std::time::Duration::from_secs_f64(approx_wait_time));
        Ok(samp_num)
    }

    pub fn write_digital_lines(&self, _samp_buf: &[u8], samp_num: usize, _timeout: Option<f64>) -> Result<usize, DAQmxError> {
        let approx_wait_time = 0.8 * 100e-9 * samp_num as f64;  // 100 ns - sample clock period assuming 10 MSa/s sampling rate for DO card
        std::thread::sleep(std::time::Duration::from_secs_f64(approx_wait_time));
        Ok(samp_num)
    }

    pub fn write_analog(&self, _samp_buf: &[f64], samp_num: usize, _timeout: Option<f64>) -> Result<usize, DAQmxError> {
        let approx_wait_time = 0.8 * 1e-6 * samp_num as f64;  // 1 us - sample clock period assuming 1 MSa/s sampling rate for AO card
        std::thread::sleep(std::time::Duration::from_secs_f64(approx_wait_time));
        Ok(samp_num)
    }

    pub fn set_ref_clk_rate(&self, _rate: f64) -> Result<(), DAQmxError> {
        Ok(())
    }

    pub fn set_ref_clk_src(&self, _src: &str) -> Result<(), DAQmxError> {
        Ok(())
    }

    pub fn cfg_ref_clk(&self, src: &str, rate: f64) -> Result<(), DAQmxError> {
        self.set_ref_clk_rate(rate)?;
        self.set_ref_clk_src(src)?;
        Ok(())
    }

    pub fn cfg_dig_edge_start_trigger(&self, _trigger_source: &str) -> Result<(), DAQmxError> {
        Ok(())
    }

    pub fn get_write_current_write_pos(&self) -> Result<u64, DAQmxError> {
        Ok(0)
    }

    pub fn export_signal(&self, _signal_id: CInt32, _output_terminal: &str) -> Result<(), DAQmxError> {
        Ok(())
    }

    pub fn get_write_total_samp_per_chan_generated(&self) -> Result<u64, DAQmxError> {
        Ok(0)
    }
}
