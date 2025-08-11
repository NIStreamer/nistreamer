# Project Structure
In essence, `nistreamer` package is an abstraction layer on top of the proprietary NI driver. It interacts with the driver via [`NI-DAQmx` C API](https://www.ni.com/docs/en-US/bundle/ni-daqmx-c-api-ref/page/cdaqmx/help_file_title.html) (see the figure below).

The package itself consists of two layers - the Rust back-end and the Python front-end. Most of the logic is actually contained in the back-end, while the front-end is a very thin layer providing a user-friendly Python API (see details below).

![Project structure schematics. Rust back-end is comprised of nistreamer-base, nistreamer, and nistreamer-usrlib crates and implements the streamer-device-channel-instruction tree structure. There is an interface layer between Rust back-end and Python front-end based on PyO3 and maturin. The user-facing Python API part is a collection of proxy classes which artificially "re-inflate" the streamer-device-channel tree which was "flattened" when passing across the language interface.](https://github.com/user-attachments/assets/bf06c51c-393c-47c9-a747-753f97a9f99d)

## Rust Back-end
The back-end is split into several crates[^1].

[^1]: In Rust, a _crate_ is the fundamental unit of compilation and packaging. A crate can be identified by locating a `Cargo.toml` file (called a _manifest_) in its root directory. ToDo: https://doc.rust-lang.org/book/ch07-01-packages-and-crates.html#packages-and-crates

**(1)** `nistreamer-base` contains base traits:
* Waveform function library base:
  * Traits `Calc<T>` and `FnTraitSet<T>` - the definition of a waveform function;
  * `FnBox` types for passing waveform objects across Python;
  * A built-in "standard library" of waveform functions.  
  
* Base traits for all hardware elements:
  * Channels (instruction collision checks, sequence compilation, and sample computation logic);
  * Devices (mainly common timing across all channels);
  * Streamer (mainly common timing across all devices).

**(2)** `nistreamer` contains specific types and all hardware configuration details:
* A Rust wrapper of NI-DAQmx C API (`nidaqmx.rs` module); 
* Concrete structs for the streamer, AO and DO channels and cards;
* Multi-threading implementation;
* The Rust-Python interface layer (flattened Rust API, PyO3 wrapper, see details below).
    
Together, `nistreamer-base` and `nistreamer` contain the bulk of the back-end logic. In addition, there are two smaller crates:

**(3)** `nistreamer-macros` contains helper procedural macros for waveform function libraries.

**(4)** `nistreamer-usrlib` is an optional dependency and provides a space for a user-editable custom waveform library. It is also split out into a separate [GitHub repository](https://github.com/NIStreamer/nistreamer-usrlib) such that users can create and maintain their own fork.

The schematic below is showing how logic is distributed between the crates:  
![Rust back-end is composed of several crates. The image shows the key modules and their functionality in each. For example, logic for channel, device, and streamer types is split between `nistreamer_base` and `nistreamer` crates. An equivalent description is provided in the main text above.](images/backend_structure.svg  "Backend structure schematic")

The crate dependency graph is shown below. `nistreamer` is the head - building it will trigger compilation of all other ones and will build the whole package. `nistreamer_usrlib` is optional and is only included when building with `--features usrlib` flag.

[//]: # (ToDo: dependency graph)

There are several reasons for such a sub-structure. First, `nistreamer-macros` has to be separate since it is a procedural macro crate. Second, `nistreamer-usrlib` should be outside the main codebase since it is optional and moreover a user-editable part. Finally, there are two reason for splitting the core logic into `nistreamer-base` and `nistreamer`:

* To avoid the cyclic dependency issue when including `nistreamer-usrlib`.  
  `nistreamer-usrlib` depends on waveform traits from `nistreamer-base`, while `nistreamer` depends on `nistreamer-usrlib` to include user library class into the final extension module in `lib.rs`. If the two core packages were a single crate, there would be a cyclic dependency.

* Such separation simplifies development. If any changes are made to `nistreamer-base`, one can attempt compiling it without having to refactor `nistreamer` yet.

## Rust-Python interface
We use a combination of [`PyO3`](https://github.com/PyO3/pyo3) and [`maturin`](https://github.com/PyO3/maturin) to build and wrap the back-end as a Python extension module. 

To expose public methods of a streamer and all contained devices and channels, we bring them together in a "flattened" manner as methods of a single `StreamerWrap` struct which is annotated as `#[pyclass]` (see `flat_wrap.rs` module). So in Python, there is only a single "monolithic" entity representing the entire streamer tree - a `StreamerWrap` instance. Calling a method on a particular channel, for example, is done through a corresponding method of `StreamerWrap` by providing the full device and channel identifiers. 

This approach minimizes the Rust-Python boundary at the cost of losing the user-friendly dot notation access to device and channel methods. It is artificially restored by the Python front-end layer (see [next section](#front-end)).  

When building with `maturin`, compiled backend is packaged as an importable DLL module `_nistreamer.pyd` which is placed directly into the front-end directory for use by the proxy classes. 

Apart from `StreamerWrap`, there are two more `#[pyclass]`es which go into `_nistreamer.pyd` - the built-in `StdFnLib` and the optional `UsrFnLib` waveform libraries which will be described in detail in a separate section.

## Front-end
The user-facing Python front-end is contained in `py_api` directory. It is a very thin layer on top of `StreamerWrap` with the main function to artificially restore the dot notation access to device and channel methods which was lost when going through the interface (see [previous section](#rust-python-interface)).

Every device and channel is represented by a corresponding proxy instance, which stores a direct reference to the central `StreamerWrap` as well as the device/channel identifiers. 
The user script is making dot-notation proxy method calls, while proxies in turn are simply calling the corresponding methods of `StreamerWrap` with their device/channel identifiers as arguments.
