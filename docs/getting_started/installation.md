# Installation
Before starting, ensure you have the official [NI-DAQmx driver](https://www.ni.com/en/support/downloads/drivers/download.ni-daq-mx.html) installed – `nistreamer` is only a layer on top of the driver and will not work without it.

## Installing with `pip`
The easiest way to get `nistreamer` is by using `pip`. Activate the desired Python environment (e.g. `conda activate ...`) and run the following command:
```
pip install nistreamer
```
This will download and install a fully pre-compiled package into your Python environment. You should be able to import and use `nistreamer` immediately: 
```Python
from nistreamer import NIStreamer, std_fn_lib
```
Next, check out quick-start {doc}`tutorials </usage/index>`.

:::{note}
A pre-compiled version is only available for Windows, x86-64/AMD64 architecture, Python 3.7 and higher. You need to build from source to run on other platforms (see instructions below).
:::

(building-from-source)=
## Building from source
You need to compile the project directly from source in the following cases:
* When adding custom waveform functions through `usrlib` option;
* Your platform is not supported by the pre-compiled distribution[^1].

The step-by-step guide is provided below.

[^1]: The project was only tested on Windows. In principle, it should also work on Linux - NI provides DAQmx drivers for Linux and `nistreamer` source code should be compatible as well. You will need to add `NIDAQmx.lib` location in `.cargo/config.toml` (see [this note](#cannot-open-input-file-nidaqmx-lib)). 

### 1. Get the tools
Install Rust by following the [official instructions](https://www.rust-lang.org/learn/get-started). This process will install all necessary Rust components including `cargo` – the command-line tool we will be using to compile the backend. You can verify that `cargo` was installed by running `cargo --version`.

We also need [maturin](https://www.maturin.rs/) to install compiled backend as a Python package. `maturin` itself is shipped as a Python package, so activate the desired environment (e.g. `conda activate ...`) where you want `nistreamer` to be eventually installed and run `pip install maturin`.

Finally, we recommend installing `git` – the command-line version control tool. This is the best way to get `nistreamer` source code you need and also pull the updates in the future. Download and run the [official installer](https://git-scm.com/downloads). Alternatively, you can manually download source code as `.zip` archive from GitHub.

### 2. Get the source code
The full source code is hosted on [GitHub](https://github.com/NIStreamer/nistreamer). Download the repository with `git`:
```
git clone https://github.com/NIStreamer/nistreamer.git
```
(or you can manually download and unpack `.zip` archive from GitHub if `git` is not installed).

To build with a library of custom waveforms, you should also get a clone of `nistreamer-usrlib` ([GitHub](https://github.com/NIStreamer/nistreamer-usrlib)) which you will be populating with new functions. We recommend creating your own GitHub fork of this repo - this way you can both version-control your own library and also pull in any future interface updates. 

Once forked, clone your `nistreamer-usrlib` next to `nistreamer` - your directory tree should look like this:
```
parent_directory/
├── nistreamer/
│   ├── Cargo.toml
│   ├── pyproject.toml
│   └── ...
└── nistreamer-usrlib/
    ├── Cargo.toml
    └── ...
```

### 3. Compile and install
Open a terminal and navigate into `nistreamer` directory (this location should contain both `pyproject.toml` and `Cargo.toml` files). Don't forger to activate the desired Python environment (e.g. `conda activate ...`).

For a default installation with the built-in waveform library run:
```
maturin develop
```
while building with the `usrlib` option requires adding the features flag[^2]:
```
maturin develop --features usrlib
```
[^2]: If building on Linux, add the `pyo3/extension-module` feature like this: `maturin develop --features "usrlib, pyo3/extension-module"`

This command will both compile the back-end and install the full package into your active Python environment. 
If this operation completed without errors, you should be able to import and use `nistreamer` immediately:
```Python
from nistreamer import NIStreamer, std_fn_lib 
from nistreamer import usr_fn_lib  # only if this feature was enabled
```   
Next, check out quick-start {doc}`tutorials </usage/index>`.

If build failed, refer to [troubleshooting guide](#troubleshooting).

### Troubleshooting
If build fails, read the error message - it contains a lot of useful information and sometimes suggests a fix to the issue.

#### Building with `usrlib`
If building with `--features usrlib` fails, try compiling without it as a sanity check. If default build works, there is likely a mistake in your Rust code.

Look through the full print-out. It will look something like this:
```
   Compiling pyo3-build-config v0.22.6
   Compiling pyo3-macros-backend v0.22.6
   ...
   ... many lines skipped ... 
   ...
   Compiling nistreamer-base v0.0.0 (C:\Users\...\nistreamer-base)
   
   Compiling nistreamer-usrlib v0.1.0 (C:\Users\...\nistreamer-usrlib)   <-- THIS STEP FAILED
   
error[E0277]: cannot add `{integer}` to `f64`   <-- ERROR TYPE, COPY FOR WEB SEARCHING

  --> C:\Users\...\nistreamer-usrlib\src\lib.rs:19:40   <-- PRICESE LOCATION
  
... 
help: consider using a floating-point literal by writing it with `.0`  <-- SUGGESTED FIX
...
```
In this example, it is indeed `nistreamer-usrlib` which failed to compile due to a mistake in custom Rust code. In such cases, see if the compiler suggested any fixes. A web search with the error type can also give solutions for the most common Rust pitfalls. 

#### Cargo is not recognized
If you installed Rust and still get the following error:
```
cargo is not recognized as an internal or external command
```
ensure that Rust binaries are added to your system's PATH. The installer typically does this, but in case it didn't, add `C:\Users\<YOUR_USERNAME>\.cargo\bin` to your system's PATH.

#### Cannot open input file `NIDAQmx.lib`
Error message ending with the following line:
```
note: LINK : fatal error LNK1181: cannot open input file 'NIDAQmx.lib'
```
means that Cargo did not find the NIDAQmx static library which has to be present on the computer to compile the backend (unless you are compiling with `nidaqmx_dummy` feature, see [advanced build options](#advanced-build-options)). On Windows, the following location of NIDAQmx library is assumed by default:
```
C:/Program Files (x86)/National Instruments/Shared/ExternalCompilerSupport/C/lib64/msvc/NIDAQmx.lib
```
If on your system it is located somewhere else, update the linker arguments in `nistreamer/.cargo/config.toml` with the correct full path.


### Advanced build options
Back-end can be compiled with optional build features[^2]:
```
maturin develop --features feature_name
```
The following features are available:

| Name            | Desctiption                                                                                                                       |
|-----------------|-----------------------------------------------------------------------------------------------------------------------------------|
| `usrlib`        | Include user function library. `nistreamer-usrlib` crate should be placed in the same directory with `nistreamer`.                |
| `nidaqmx_dummy` | Compile with a "dummy" instead of the actual NI-DAQmx driver. Helpful for development on a computer without the driver installed. |

Multiple features can be combined: `--features "usrlib, nidaqmx_dummy"`
