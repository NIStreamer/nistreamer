//! Provides definitions and implementations for instruction-related functionalities.
//!
//! ## Main Structures and Enumerations:
//!
//! - [`InstrType`]: An enumeration that defines the types of instructions supported, including `CONST` for constant values and `SINE` for sinusoidal waves.
//!
//! - [`Instruction`]: Represents a general instruction composed of a type (`InstrType`) and a set of arguments (`InstrArgs`). It offers methods for creating specific instruction types conveniently and for evaluating them.
//!
//! - [`InstrBook`]: Manages an instruction along with its associated metadata during the experiment editing phase, capturing details like the defined interval and whether to retain a value after the defined interval.
//!
//! ## Utilities:
//!
//! - The `InstrArgs` type alias provides a convenient way to define instruction arguments using a dictionary with string keys and float values.
//!
//! - The module makes use of the `maplit` crate to enable easy creation of IndexMaps.
//!
//! ## Features:
//!
//! - Easy creation of instruction objects with utility methods such as `new_const` and `new_sine`.
//! - Ability to evaluate instructions and in-place populate given time array views with the resulting float-point values.
//! - Support for default values in instructions, allowing for flexibility and ease of use.

use std::cmp::Ordering;
use std::fmt;
use std::fmt::Display;
use crate::fn_lib_tools::FnTraitSet;

/// Struct containing function and start/end edge data of the instruction.
///
/// # Fields:
/// - `func` - the function struct
///
/// - `start_pos` - beginning of the instruction interval
///
/// - `end_spec` specifies instruction interval end. Can be `Some` or `None`:
///     - `Some((end_pos, keep_val))` - instruction has specific `end_pos`.
///       If there is a gap until the next edge (the next instruction or global end), compiler will keep a constant value starting at `end_pos`.
///       If `keep_val` is `true`, it will be the last instruction value, otherwise it will be the channel default.
///
///     - `None` - no specified end, instruction will span until the next edge (start of the next instruction or global end).
///
/// # Edge inclusion:
/// - `start_pos` is *inclusive*, sample for `start_pos` clock tick is covered;
/// - `end_pos` is *exclusive*, sample for `end_pos` clock tick is not covered, the next instruction can start here otherwise it will be covered by padding;
///
/// # Minimal instruction length is 1 clock tick:
/// - If `end_spec` is `Some`, minimal `end_pos` is `start_pos + 1`
/// - If `end_spec` is `None`, the next instruction must start no earlier than `start_pos + 1`
///
/// # Ordering
/// `Instr` implements ordering based on `start_pos` to facilitate sorting.
///
pub struct Instr<T> {
    start_pos: usize,
    end_spec: Option<(usize, bool)>,
    func: Box<dyn FnTraitSet<T>>,
}
impl<T> Instr<T> {
    /// Constructs a new `InstrBook` object.
    ///
    /// Checks that `end_pos` is strictly greater than `start_pos`.
    ///
    /// # Arguments
    /// - `start_pos`: Starting position (inclusive).
    /// - `end_spec`: specifies instruction interval end. Can be `Some` or `None`:
    ///     - `Some((end_pos, keep_val))` - instruction has specific `end_pos`.
    ///       If there is a gap until the next edge (the next instruction or global end), compiler will keep a constant value starting at `end_pos`.
    ///       If `keep_val` is `true`, it will be the last instruction value, otherwise it will be the channel default.
    ///     - `None` - no specified end, instruction will span until the next edge (start of the next instruction or global end).
    /// - `func`: The associated function.
    ///
    /// # Examples
    ///
    /// Constructing a valid `InstrBook`:
    ///
    /// ```
    /// # use nicompiler_backend::instruction::*;
    /// let instruction = Instruction::new(InstrType::CONST, [("value".to_string(), 1.0)].iter().cloned().collect());
    /// let book = InstrBook::new(0, Some((5, true)), instruction);
    /// ```
    ///
    /// Attempting to construct an `InstrBook` with `end_pos` not greater than `start_pos` will panic:
    ///
    /// ```should_panic
    /// # use nicompiler_backend::instruction::*;
    /// let instruction = Instruction::new(InstrType::CONST, [("value".to_string(), 1.0)].iter().cloned().collect());
    /// let book = InstrBook::new(5, Some((5, true)), instruction);
    /// ```
    ///
    /// The panic message will be:
    /// `Instruction { /* ... */ } end_pos 5 should be strictly greater than start_pos 5`.
    pub fn new(start_pos: usize, end_spec: Option<(usize, bool)>, func: Box<dyn FnTraitSet<T>>) -> Self {
        if let Some((end_pos, _keep_val)) = &end_spec {
            // Sanity check - the smallest permissible instruction length is 1 tick
            assert!(
                start_pos + 1 <= *end_pos,
                "Instruction must satisfy `start_pos + 1 <= end_pos` \n\
                 However, provided instruction has start_pos = {start_pos} and end_pos = {end_pos}"
            )
        }
        Instr {
            start_pos,
            end_spec,
            func,
        }
    }
    /// Returns the value of the `start_pos` field
    pub fn start_pos(&self) -> usize {
        self.start_pos
    }
    pub fn start_pos_mut(&mut self) -> &mut usize {
        &mut self.start_pos
    }
    /// Returns the value of the `end_spec` field
    pub fn end_spec(&self) -> Option<(usize, bool)> {
        self.end_spec.clone()
    }
    pub fn end_spec_mut(&mut self) -> &mut Option<(usize, bool)> {
        &mut self.end_spec
    }
    /// Returns the value of the `end_pos` sub-field
    pub fn end_pos(&self) -> Option<usize> {
        match self.end_spec {
            Some((end_pos, _keep_val)) => Some(end_pos),
            None => None,
        }
    }
    /// Returns the value of the `keep_val` sub-field
    pub fn keep_val(&self) -> Option<bool> {
        match self.end_spec {
            Some((_end_pos, keep_val)) => Some(keep_val),
            None => None,
        }
    }
    /// "Effective" end position
    ///
    /// If `Self.end_spec` is `Some`, simply returns `end_pos`.
    ///
    /// If `Self.end_spec` is `None`, returns `(start_pos + 1)`.
    /// This is because "go-something" instruction must have at least one tick - `start_pos` - to have any effect.
    /// So the "effective end", the earliest time any subsequent instruction can be starting at is `(start_pos + 1)`.
    pub fn eff_end_pos(&self) -> usize {
        // "go_something"-type instruction don't have a specific end_pos
        // but must have space for at least one tick to have any effect,
        // so the closest permissible end_pos is (start_pos + 1)
        match self.end_pos() {
            Some(end_pos) => end_pos,
            None => self.start_pos + 1,
        }
    }
    /// Returns `Some(end_pos - start_pos)` or `None` if not specified
    pub fn dur(&self) -> Option<usize> {
        match self.end_spec {
            Some((end_pos, _keep_val)) => Some(end_pos - self.start_pos),
            None => None,
        }
    }

    pub fn func(&self) -> &Box<dyn FnTraitSet<T>> {
        &self.func
    }
}

// Support total ordering for Instr
impl<T> Ord for Instr<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        // We reverse the order to make BinaryHeap a min-heap based on start_pos
        self.start_pos.cmp(&other.start_pos)
    }
}
impl<T> PartialOrd for Instr<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> PartialEq for Instr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.start_pos == other.start_pos
    }
}
impl<T> Eq for Instr<T> {}

impl<T> Display for Instr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let end_spec = match self.end_spec {
            Some((end_pos, keep_val)) => format!("end_pos={end_pos}, keep_val={keep_val}"),
            None => "no specified end".to_string(),
        };
        write!(
            f,
            "Instr(func={:?}, start_pos={}, {})",  // ToDo: a way to implement Display for Box<dyn FnTraitSet>
            self.func, self.start_pos, end_spec
        )
    }
}
