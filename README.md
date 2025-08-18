# NI Pulse Streamer

An abstraction layer providing a Python API for scripted pulse sequence generation with [National Instruments](http://www.ni.com) hardware.

**Main features:**

* Simple Python API allows scripting very complex sequences.

* Streaming approach enables practically unlimited sequence duration. The pulse sequence is efficiently stored as a list of instructions, while the waveform samples are computed on the fly, requiring only a small amount of memory at any given time.

* The streaming back-end is implemented in Rust – fast, lightweight, and robust.

* Versatile package format – the streamer can be run as a standalone tool with a minimal Python script, or be integrated into any other control software.

See [project documentation](https://nistreamer.readthedocs.io) for more details. In particular, the `Internals/Project Structure` section explains source code layout.
