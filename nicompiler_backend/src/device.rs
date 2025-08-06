//! Implements struct and methods corresponding to NI devices. See [`BaseDevice`] for
//! implementation details.
//!
//! A NI control system consists of one or both of the components:
//! 1. Devices (cards) directly attached the computer via PCIe/USB.
//! 2. A PCIe link card connected to a PXIe chassis, which hosts multiple PXIe cards.
//!
//! ## Device
//! In this library, every [`Device`] object corresponds to a particular task for
//! a physical device (e.g. analogue output for `PXI1Slot1`). A `Device` trivially implements the
//! [`BaseDevice`] trait by supplying field methods.
//!
//! [`Device`] fields keep tracks of of the physical channels associated with the device
//! as well as device-wide data such as device name, trigger line, and synchronization behavior.
//!
//! The [`Device`] struct is the primary structure used to interact with NI hardware. It groups multiple
//! channels, each of which corresponds to a physical channel on an NI device. This struct provides
//! easy access to various properties of the device, such as its physical name, task type, and
//! several clock and trigger configurations.
//! For editing and compiling behavior of devices, see the [`BaseDevice`] trait.
//!
//!
//! ### Editable and streamable channels in devices
//! Library users create and edit editable channels. During compilation, based on the device's task type,
//! the library may internally add streamable channels.
//! For more details on editable and streamable channels, see the editable v.s. streamable section in
//! [`channel` module].
//!
//! ### Synchronization methods for devices
//! Each device's synchronization behavior is specified by its constructor arguments.
//! Refer to the [`Device`] struct for a more detailed explanation.
//!
//! [`channel` module]: crate::channel

use ndarray::Array1;
use indexmap::IndexMap;
use itertools::Itertools;
use crate::channel::BaseChan;

/// The `BaseDevice` trait defines the fundamental operations and attributes of a National Instruments (NI) device.
///
/// This trait abstracts the common functionalities that an NI device should possess, regardless of its specific hardware details or task type. Implementers of this trait will have access to core functionalities like channel management, device status checks, signal compilation, and more.
///
/// ## Typical Use
///
/// A type implementing `BaseDevice` is primarily used to interact with the associated NI hardware, manage its channels, and perform operations like signal generation, editing, and compilation.
///
/// # Trait Methods and Their Functionality:
///
/// - **Field methods**: These provide direct access to the properties of a device, such as its channels, physical name,
/// sampling rate, and various configuration parameters.
///
/// - **Synchronization configuration**: Customize the synchronization behavior of devices via [`BaseDevice::cfg_trig`],
/// [`BaseDevice::cfg_ref_clk`], [`BaseDevice::cfg_samp_clk_src`]. See [`Device`] for more details.
///
/// - **Channel management**: Methods like [`BaseDevice::editable_channels`], [`BaseDevice::editable_channels_`], and
/// [`BaseDevice::add_channel`] allow for the retrieval and manipulation of channels associated with the device.
///
/// - **Device status checks**: Methods like [`BaseDevice::is_compiled`], [`BaseDevice::is_edited`], and
/// [`BaseDevice::is_fresh_compiled`] enable checking the compilation and editing status of the device's channels.
///
/// - **Cache operations**: The methods [`BaseDevice::clear_edit_cache`] and [`BaseDevice::clear_compile_cache`] are
/// used to clear the edit and compile caches of the device's channels, respectively.
///
/// - **Compilation**: The [`BaseDevice::compile`] method takes care of the signal compilation process for the device's
/// channels. For Digital Output (DO) channels, it provides additional functionality to merge line channels into port channels.
///
/// - **Signal generation**: The [`BaseDevice::fill_signal_nsamps`] and [`BaseDevice::calc_signal_nsamps`] methods are
/// central to signal generation, allowing for the sampling of float-point values from compiled instructions based on
/// various criteria.
///
/// - **Utility functions**: Methods like [`BaseDevice::unique_port_numbers`] offer utility functionalities specific to certain
/// task types, aiding in operations like identifying unique ports in Digital Output (DO) devices.
///
///
/// # Implementing [`BaseDevice`]:
///
/// When creating a new type that represents an NI device, implementing this trait ensures that the type has all the necessary methods and behaviors typical of NI devices. Implementers can then extend or override these methods as necessary to provide device-specific behavior or optimizations.
pub trait BaseDev {
    /// Output channel type
    type Chan: BaseChan;

    // Field methods
    fn name(&self) -> String;
    fn samp_rate(&self) -> f64;

    fn chans(&self) -> Vec<&Self::Chan>;
    fn chans_mut(&mut self) -> Vec<&mut Self::Chan>;

    /// Shortcut to borrow channel instance by name
    fn chan(&self, name: &str) -> Result<&Self::Chan, String> {
        let search_idx = self.chans().iter().position(|chan| chan.name() == name.to_string());

        if let Some(idx) = search_idx {
            Ok(self.chans().swap_remove(idx))
        } else {
            Err(format!("Device {} does not have channel {name}", self.name()))
        }
    }
    /// Shortcut to mutably borrow channel instance by name
    fn chan_mut(&mut self, name: &str) -> Result<&mut Self::Chan, String> {
        let search_res = self.chans().iter().position(|chan| chan.name() == name.to_string());

        if let Some(idx) = search_res {
            Ok(self.chans_mut().swap_remove(idx))
        } else {
            Err(format!("Device {} does not have channel {name}", self.name()))
        }
    }

    /// Returns sample clock period calculated as `1.0 / self.samp_rate()`
    fn clk_period(&self) -> f64 {
        1.0 / self.samp_rate()
    }

    /// Adds a new channel to the device.
    fn check_can_add_chan(&mut self, chan: &Self::Chan) -> Result<(), String> {
        if f64::abs(chan.samp_rate() - self.samp_rate()) >= 1e-10 {
            return Err(format!(
                "Cannot add channel {} with samp_rate={} to device {} with a different samp_rate={}",
                chan.name(), chan.samp_rate(), self.name(), self.samp_rate()
            ))
        };
        let chan_names: Vec<_> = self.chans().iter().map(|chan| chan.name()).collect();
        if chan_names.contains(&chan.name()) {
            return Err(format!(
                "There is already a channel with name {} registered. Registered channels are {:?}",
                chan.name(), chan_names
            ))
        };
        Ok(())
    }

    fn add_reset_instr(&mut self, reset_time: f64) -> Result<(), String> {
        let reset_pos = (reset_time * self.samp_rate()).round() as usize;

        // Sanity check - reset_pos does not clip any existing instructions
        if self.last_instr_end_pos().is_some_and(|last_instr_end| reset_pos < last_instr_end) {
            return Err(format!(
                "[Device {}] given reset_time {reset_time} was rounded to {reset_pos} clock cycles \
                which is below the last instruction end position {}",
                self.name(), self.last_instr_end_pos().unwrap()
            ))
        }

        for chan in self.chans_mut() {
            chan.add_reset_instr(reset_pos)?
        };
        Ok(())
    }

    /// A device is marked edited if any of its editable channels are edited.
    /// Also see [`BaseChannel::is_edited`]
    fn got_instructions(&self) -> bool {
        self.chans().iter().any(|chan| chan.got_instructions())
    }

    fn active_chans(&self) -> Vec<&Self::Chan> {
        self.chans()
            .drain(..)
            .filter(|chan| chan.got_instructions())
            .collect()
    }

    fn active_chans_mut(&mut self) -> Vec<&mut Self::Chan> {
        self.chans_mut()
            .drain(..)
            .filter(|chan| chan.got_instructions())
            .collect()
    }

    /// Clears the edit-cache fields for all channels.
    /// Also see [`BaseChannel::clear_edit_cache`]
    fn clear_edit_cache(&mut self) {
        for chan in self.chans_mut() {
            chan.clear_edit_cache()
        }
        self.clear_compile_cache();
    }
    /// Clears the compile-cache fields for all channels.
    /// Also see [`BaseChannel::clear_compile_cache`]
    fn clear_compile_cache_base(&mut self) {
        for chan in self.chans_mut() {
            chan.clear_compile_cache()
        }
    }

    fn clear_compile_cache(&mut self) {
        self.clear_compile_cache_base()
    }

    fn is_closing_edge_clipped(&self, stop_tick: usize) -> bool {
        if self.last_instr_end_pos().is_some_and(|last_end_pos| stop_tick < last_end_pos) {
            panic!("Given stop_tick {stop_tick} is below the last instruction end_pos {}", self.last_instr_end_pos().unwrap())
        }
        self.chans()
            .iter()
            .filter_map(|chan| chan.instr_list().last())
            .any(|last_instr| {
                match last_instr.end_pos() {
                    Some(end_pos) => stop_tick == end_pos,
                    None => false,
                }
            })
    }

    /// Compiles all editable channels to produce a continuous instruction stream.
    ///
    /// The method starts by compiling each individual editable channel to obtain a continuous
    /// stream of instructions (also see[`BaseChannel::compile`]).
    /// If the device type is `TaskType::DO` (Digital Output), an additional
    /// processing step is performed. All the line channels belonging to the same port are merged
    /// into a single, streamable port channel that is non-editable. This aggregated port channel
    /// contains constant instructions whose integer values are determined by the combined state
    /// of all the lines of the corresponding port. Specifically, the `n`th bit of the integer
    /// value of the instruction corresponds to the boolean state of the `n`th line.
    ///
    /// # Port Channel Aggregation
    /// Each instruction inside the aggregated port channel is a constant instruction. The value of
    /// this instruction is an integer, where its `n`th bit represents the boolean state of the
    /// `n`th line. This way, the combined state of all lines in a port is efficiently represented
    /// by a single integer value, allowing for streamlined execution and efficient data transfer.
    ///
    /// # Arguments
    /// - `stop_time`: The stop time used to compile the channels.
    fn compile_base(&mut self, stop_time: f64) -> Result<(), String> {
        if !self.got_instructions() {
            // @Backend developers: whenever iterating over devices, you should always
            // filter by `got_instructions()` to only interact with active devices.
            return Err(format!("Device {} did not get any instructions", self.name()))
        }
        let stop_tick = (stop_time * self.samp_rate()).round() as usize;
        if stop_tick < self.last_instr_end_pos().unwrap() {
            return Err(format!(
                "[Device {}] requested stop_time {stop_time} was rounded to {stop_tick} clock cycles \
                which is below the last instruction end_pos {}",
                self.name(), self.last_instr_end_pos().unwrap()
            ))
        }

        // If on any of the channels, the last instruction has `end_spec = Some(end_pos, ...)`
        // and requested `stop_tick` precisely matches `end_pos`,
        // we ask the card to generate an additional sample at the end to ensure this "closing edge" is reliably formed.
        //
        // Explanation:
        // If there were no extra sample, generation will simply stop at the last sample of the pulse
        // and what happens next would be hardware-dependent. Specifically NI cards simply keep the last generated value,
        // resulting in the pulse having the first "opening" edge, but not having the second "closing" edge.
        //
        // To avoid this issue (and any similar surprises for other hardware platforms),
        // we explicitly ask the card to run for one more clock cycle longer and generate the extra sample at the end.
        // Channel's `compile()` logic will fill this sample with the last instruction's after-end padding
        // thus reliably forming its' "closing edge".
        let stop_pos = if self.is_closing_edge_clipped(stop_tick) {
            stop_tick + 1
        } else {
            stop_tick
        };

        // Compile all active channels
        for chan in self.active_chans_mut() {
            chan.compile(stop_pos)?
        };

        Ok(())
    }

    fn compile(&mut self, stop_time: f64) -> Result<(), String> {
        self.compile_base(stop_time)
    }

    /// Base of `validate_compile_cache()`
    fn validate_compile_cache_base(&self) -> Result<(), String> {
        // 3 checks:
        // - this device is active in the first place;
        // - each active channels passes `validate_compile_cache()` test (meaning it is "fresh compiled" - compile cache matches current edit cache);
        // - and they should all be compiled to the same stop position (since all channels of a given device share the same sample clock)

        if !self.got_instructions() {
            // @Backend developers: whenever iterating over devices, you should always
            // filter by `got_instructions()` to only interact with active devices.
            return Err(format!("Device {} does not have any instructions and is not active", self.name()))
        }

        let failed_chan_msgs: Vec<String> = self
            .active_chans()
            .iter()
            .filter_map(|chan| chan.validate_compile_cache().err())
            .collect();
        if !failed_chan_msgs.is_empty() {
            let mut full_err_msg = String::new();
            for msg in failed_chan_msgs {
                full_err_msg.push_str(&format!("{msg}\n"))
            };
            return Err(format!("[{}] The following channels failed compile cache validation:\n{full_err_msg}", self.name()))
        }

        let compiled_stop_positions: IndexMap<String, usize> = self
            .active_chans()
            .iter()
            .map(|chan| (chan.name(), chan.compiled_stop_pos()))
            .collect();
        if !compiled_stop_positions.values().all_equal() {
            return Err(format!("[{}] Channels have different compiled stop positions: \n{compiled_stop_positions:?}", self.name()))
        }

        Ok(())
    }

    /// Ensures that compile cache is fresh (matches current edit cache) and is self-consistent
    fn validate_compile_cache(&self) -> Result<(), String> {
        self.validate_compile_cache_base()
    }

    /// Returns the total number of samples the card will generate according to the current compile cache.
    fn compiled_stop_pos(&self) -> usize {
        // Sanity checks:
        if !self.got_instructions() {
            panic!(
                "Device {} hasn't gotten any instructions yet and is currently inactive.\n\
                \n\
                @Backend developers: when iterating over devices, you should always filter by `got_instructions()` and skip inactive ones",
                self.name()
            )
        }
        if let Err(msg) = self.validate_compile_cache() {
            panic!(
                "{msg}\n\
                \n\
                @Backend developers: whenever accessing compile cache, you should first call `validate_compile_cache()` \
                to ensure that compile cache is valid - up-to-date with the edit cache and has no inconsistencies. \n\
                \n\
                This function is meant to be the place to gracefully handle the Err variant if it occurs \
                (typically due to users forgetting to re-compile after adding pulses). \n\
                \n\
                In contrast, other functions assume the cache is valid and rely on it. Some may still \
                do a 'validate_compile_cache()' under the hood to catch bugs but they will panic on Err."
            )
        }

        self.active_chans()
            .last()
            .unwrap()
            .compiled_stop_pos()
    }

    /// Calculates the maximum stop time among all compiled channels.
    ///
    /// Iterates over all the compiled channels in the device, regardless of their streamability or
    /// editability, and determines the maximum stop time.
    /// See [`BaseChannel::total_run_time`] for more information.
    ///
    /// # Returns
    /// A `f64` representing the maximum stop time (in seconds) across all compiled channels.
    fn compiled_stop_time(&self) -> f64 {
        self.compiled_stop_pos() as f64 * self.clk_period()
    }

    fn last_instr_end_pos(&self) -> Option<usize> {
        self.chans()
            .iter()
            .filter_map(|chan| chan.last_instr_end_pos())
            .reduce(
                |largest_so_far, this| std::cmp::max(largest_so_far, this)
            )
    }

    /// Calculates the maximum stop time among all editable channels and optionally adds an extra tick duration.
    ///
    /// This function determines the maximum stop time by iterating over all editable channels. 
    /// If `extra_tail_tick` is `true`, an additional duration, equivalent to one tick of the device's 
    /// sampling rate, is added to the maximum stop time.
    ///
    /// See [`BaseChannel::edit_stop_time`] for how individual channel stop times are determined.
    ///
    /// # Returns
    /// A `f64` representing the maximum stop time (in seconds) across all editable channels, 
    /// optionally increased by the duration of one tick.
    fn last_instr_end_time(&self) -> Option<f64> {
        self.last_instr_end_pos().map(|end_pos| end_pos as f64 * self.clk_period())
    }

    /// Computes and returns the signal values for specified channels in a device.
    ///
    /// This method calculates the signal values by sampling float-point values from compiled instructions
    /// of the device's channels. Depending on the requirements, the signal can be either intended for actual
    /// driver writing or for debugging editing intentions. For AO (Analog Output) devices, the returned buffer
    /// will contain time data.
    ///
    /// # Arguments
    /// - `start_pos`: The starting position in the sequence of compiled instructions.
    /// - `end_pos`: The ending position in the sequence of compiled instructions.
    /// - `nsamps`: The number of samples to generate.
    /// - `require_streamable`: If `true`, only signals from channels marked as streamable will be generated.
    /// - `require_editable`: If `true`, signals will be generated according to editing intentions for debugging purposes.
    ///
    /// # Returns
    /// A 2D array with the computed signal values. The first axis corresponds to the channel index and the
    /// second axis corresponds to the sample index.
    ///
    /// # Panics
    /// This method will panic if:
    /// - There are no channels that fulfill the provided requirements.
    /// - The device's task type is not AO (Analog Output) when initializing the buffer with time data.
    fn calc_samps(&self, samp_buf: &mut [<Self::Chan as BaseChan>::Samp], start_pos: usize, end_pos: usize) -> Result<(), String> {
        // Sanity checks
        //  Do not launch panics in this function since it is used during streaming runtime. Return `Result::Err` instead.
        /*      During streaming, there is an active connection to the hardware driver.
                In case of panic, context is being dropped in unspecified order.
                The connection drop logic may be invoked only after some parts of memory have already been deallocated
                and thus fail to free-up hardware properly leading to unpredictable consequences like OS freezes.
        */
        if !self.got_instructions() {
            return Err(format!("calc_samps(): device {} did not get any instructions", self.name()))
        }
        self.validate_compile_cache()?;

        if !(end_pos >= start_pos + 1) {
            return Err(format!("calc_samps(): requested start_pos={start_pos} and end_pos={end_pos} are invalid - end_pos must be no less than start_pos + 1"))
        }

        if !(end_pos <= self.compiled_stop_pos()) {
            return Err(format!("calc_samps(): requested end_pos={end_pos} exceeds the compiled stop position {}", self.compiled_stop_pos()))
        }

        let n_chans = self.active_chans().len();
        let n_samps = end_pos - start_pos;
        if n_chans * n_samps > samp_buf.len() {
            return Err(format!(
                "calc_samps(): provided samp_buf has insufficient size:\n\
                \t n_chans*n_samps={} exceeds samp_buf.len()={}",
                n_chans * n_samps, samp_buf.len()
            ))
        }

        let start_t = start_pos as f64 * self.clk_period();
        let end_t = (end_pos - 1) as f64 * self.clk_period();
        let t_arr = Array1::linspace(start_t, end_t, n_samps);
        let t_arr_slice = t_arr.as_slice().expect("[BaseDev::calc_samps()] BUG: t_arr.as_slice() returned None");

        for (chan_row_idx, chan) in self.active_chans().iter().enumerate() {
            chan.fill_samps(
                start_pos,
                &mut samp_buf[chan_row_idx * n_samps .. (chan_row_idx + 1) * n_samps],
                &t_arr_slice
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::device::*;
    use crate::instruction::*;

    /*
    pub struct Device {
        chans: IndexMap<String, Channel>,
        name: String,
        samp_rate: f64,
    }

    impl Device {
        /// Constructs a new `Device` instance.
        ///
        /// This constructor initializes a device with the provided parameters. The `channels` field
        /// is initialized as an empty collection. All synchronization fields are initialized to `None`
        /// by default. For nontrivial synchronization behavior, use the methods [`BaseDevice::cfg_samp_clk_src`],
        /// [`BaseDevice::cfg_trig`], and [`BaseDevice::cfg_ref_clk`].
        ///
        /// # Arguments
        /// - `name`: Name of the device as seen by the NI driver.
        /// - `task_type`: The type of task associated with the device.
        /// - `samp_rate`: Desired sampling rate in Hz.
        ///
        /// # Returns
        /// A new instance of `Device` with the specified configurations and all synchronization-related fields set to `None`.
        pub fn new(name: &str, task_type: TaskType, samp_rate: f64) -> Self {
            Self {
                channels: IndexMap::new(),

                name: name.to_string(),
                task_type,
                samp_rate,

                // ToDo: move this to NIStreamer crate:
                start_trig_in: None,
                start_trig_out: None,
                samp_clk_in: None,
                samp_clk_out: None,
                ref_clk_in: None,
                min_bufwrite_timeout: Some(5.0),
            }
        }
    }
    */

    #[test]
    fn last_instr_end_pos() {
        let mut dev = Device::new("Dev1", TaskType::AO, 1e3);
        dev.add_channel("ao0", 0.0);
        dev.add_channel("ao1", 0.0);
        let mock_func = Instruction::new_const(0.0);

        // No instructions
        assert_eq!(dev.last_instr_end_pos(), 0);

        // Instruction t=0..1 on ao0
        dev.chan_("ao0").add_instr(mock_func.clone(),
            0.0, Some((1.0, false))
        );
        assert_eq!(dev.last_instr_end_pos(), 1000);

        // Instruction t=1..2 on ao1
        dev.chan_("ao1").add_instr(mock_func.clone(),
            1.0, Some((1.0, false))
        );
        assert_eq!(dev.last_instr_end_pos(), 2000);

        // "Go-something" instruction on ao1 at t=2
        dev.chan_("ao1").add_instr(mock_func.clone(),
            2.0, None
        );
        assert_eq!(dev.last_instr_end_pos(), 2001);

        dev.clear_edit_cache();
        assert_eq!(dev.last_instr_end_pos(), 0);
    }

    #[test]
    fn check_end_clipped() {
        let mut dev = Device::new("Dev1", TaskType::AO, 1.0);
        dev.add_channel("ao0", 0.0);
        let mock_func = Instruction::new_const(0.0);

        // (1) No instructions
        assert_eq!(dev.check_end_clipped(0), false);

        // (2) Finite duration instruction t = 0..1s:
        //      start_pos = 0
        //      end_pos = 1
        dev.chan_("ao0").add_instr(mock_func.clone(),
            0.0, Some((1.0, false))
        );
        assert_eq!(dev.chan("ao0").last_instr_end_pos(), 1);
        assert_eq!(dev.check_end_clipped(2), false);
        assert_eq!(dev.check_end_clipped(1), true);
        dev.clear_edit_cache();

        // (3) "Go-something" instruction at t = 0s:
        //      start_pos = 0
        //      eff_end_pos = 1
        dev.chan_("ao0").add_instr(mock_func.clone(),
            0.0, None
        );
        assert_eq!(dev.chan("ao0").last_instr_end_pos(), 1);
        //  A "go-something" instruction is not meant to have the "closing" edge
        //  so setting `stop_tick` to precisely `eff_end_pos` is not considered clipping
        assert_eq!(dev.check_end_clipped(1), false);
    }

    #[test]
    fn compile() {
        let mut dev = Device::new("Dev1", TaskType::AO, 1e3);
        dev.add_channel("ao0", 0.0);
        dev.add_channel("ao1", 0.0);
        let mock_func = Instruction::new_const(0.0);

        // Not compiled yet
        assert_eq!(dev.total_samps(), 0);

        // Add some instructions on both channels
        dev.chan_("ao0").add_instr(mock_func.clone(),
            0.0, Some((1.0, false))
        );
        dev.chan_("ao1").add_instr(mock_func.clone(),
            1.0, Some((1.0, false))
        );
        assert_eq!(dev.last_instr_end_pos(), 2000);

        // Compile without clipping of the "closing edge" - no extra sample should be added
        dev.compile(3.0);
        assert_eq!(dev.total_samps(), 3000);

        // Compile with stop_pos matching the end of a finite-duration instruction on "ao1" -
        //  an additional sample should be added to form the "closing edge"
        dev.compile(2.0);
        assert_eq!(dev.total_samps(), 2001);
    }
}