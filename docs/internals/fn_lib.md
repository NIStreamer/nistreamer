# Extensible Waveform Library
To generate the signal, streamer computes output values for a grid of sample clock time points. 
For each instruction, values are computed by evaluating the contained mathematical function 
(polynomial, sine, exponent, and so on) with specified parameters. 
We interchangeably call them "waveform functions", "waveforms", or simply "functions".

The main challenge is that waveform library has to be written in Rust 
(sample computation must be fast), but at the same time it needs to be _extensible_. 

As project is evolving, more functions will be added. Moreover, users may need to introduce custom 
functions for their specific applications. There should be a clean way of adding new waveforms 
without making any changes to the codebase core.

This section describes the implemented library mechanism.

## Waveforms as trait objects
**Problem 1** - treating diverse waveforms in a uniform way.

We represent each waveform by a struct with function parameters like this:
```Rust
/// Linear function:
/// `LinFn(t) = slope*t + offs`
pub struct LinFn {
    slope: f64,
    offs: f64
}
```
So technically, different waveforms are different types. Be we need to threat them uniformly. 
For example, to contain them in the same edit cache, in the first place.

A simple approach is to [use enums](https://doc.rust-lang.org/book/ch08-01-vectors.html#using-an-enum-to-store-multiple-types).
However, it would require making explicit edits throughout the core every time
a new waveform is added to match on a new possible variant.

A more natural and scalable approach is treating waveforms as [trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html)
since all functions are unified by a single common behavior we rely on - computing signal array for a given time array.
We formalize this behavior as the following trait:
```Rust
pub trait Calc<T> {
    fn calc(&self, t_arr: &[f64], res_arr: &mut [T]);
}
```
where `T` is the output sample type (`f64` for AO, `bool` for DO). 
Each waveform must implement this trait, essentially providing the mathematical expression for computing it:
```Rust
impl Calc<f64> for LinFn {
    fn calc(&self, t_arr: &[f64], res_arr: &mut[f64]) {
        for (res, &t) in res_arr.iter_mut().zip(t_arr.iter()) {
            *res = self.slope * t + self.offs
        }
    }
}
```
Practically, more assumptions are needed, so the actual trait in use is called 
`FnTraitSet<T>` and is derived from `Calc<T>` by requiring additional technical traits 
(see `nistreamer-base/src/fn_lib_base.rs` for details). 

That way all functions are treated uniformly as trait objects:
```Rust
Box<dyn FnTraitSet<T>>
```
and expanding function library does not require any additional changes anywhere.

## Passing functions across Python
**Problem 2** - how to relay which function we want to add.

## User-editable library
Problem 3 - a place for custom waveforms.

## Helper macros
Problem 4 - minimize the amount of Rust code to write when adding custom functions.

