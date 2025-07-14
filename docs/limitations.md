# Main limitations

## Sampling rate

Maximal sampling rate is limited by NI hardware specifications. Refer to the data sheets for details.
  
  | Model (output type)    | Limit                                                                                   |
  |------------------------|-----------------------------------------------------------------------------------------|
  | PCIe/PXIe-6738 (AO)    | Either all 32 channels at up to 400 kSa/s or only 8 channels at up to 1 MSa/s           |
  | PCIe/PXIe-6535 (DO)    | All 32 channels at up to 10 MSa/s                                                       |
  | PCIe/PXIe-6363 (AO/DO) | DO - up to 10 MSa/s. AO - all 4 channels at up to 1.25 MSa/s, higher for fewer channels |

## Outputs only

Both Analog and Digital **output** channels are supported. Streamer does not have any support of **input** channels.

Your application can still read inputs by creating input tasks directly through NI DAQmx Python API (you can use [PyDAQmx](https://pythonhosted.org/PyDAQmx/) or [nidaqmx](https://nidaqmx-python.readthedocs.io/en/stable/)). This will definitely work if you have extra cards, which are not controlled by the streamer and can be dedicated for inputs. Using the same card for both streamer outputs and some inputs might be possible, but challenging since the corresponding tasks will likely conflict because of using the same on-board clock/trigger circuits.

## Only waveforms from the library

Each pulse is specified by a start time/duration and the _waveform function_. The streamer can only play waveform functions defined in its library – somewhat analogous to a [function generator](https://en.wikipedia.org/wiki/Function_generator) as opposed to an [arbitrary waveform generator](https://en.wikipedia.org/wiki/Arbitrary_waveform_generator). 

Streamer has a built-in library with some commonly used mathematical functions. Users can also add arbitrary custom functions using the optional `usr_fn_lib` feature. It requires writing a minimal amount of Rust code and re-compiling the backend, but we tried to make it maximally simple.

## Risk of underrun

The downside of streaming is the risk of buffer underrun. Samples for every subsequent chunk are being computed on the fly while the current one is playing. If this computation takes too long, the buffer underflows and generation is disrupted.

The underrun probability can be made very small, such that you practically don't encounter it. Specifically, you can adjust the `NIStreamer.chunksize_ms` property – increasing it makes streaming more stable, but the initial chunk computation will be taking longer. You should also avoid running anything else on this computer since other processes can cause occasional resource surges resulting in underruns.

If necessary, you can try to effectively disable streaming – set `NIStreamer.chunksize_ms` to be equal or greater than the full sequence duration. The entire sequence will then be sampled before generation starts. But keep in mind that setting an unreasonably long chunk size may lead to undesirable side effects – it will take longer to start and may require a huge amount of RAM to store all the samples. The in-stream looping feature will also not work since a sanity check disables it when the waveform is "too short" – shorter than the chunk size.

## No conditional branching

Streaming also means there is no simple way of fast conditional branching – changing the pulse sequence at runtime depending on external signals. Fundamentally, the waveform has to be always known for at least for the whole buffer duration ahead (typically about 100 ms) for pre-computing and transferring the samples in time. Any kind of branching here would be impractical for most use cases due to this large "response latency". So no branching is supported – the entire waveform has to be determined and compiled before generation starts.

Some limited branching can still be done by hacks. For example, one can use different channels to play the alternative sequences and use a physical switch to select between them. Alternatively, one can configure the streamer to use an external sample clock signal and gate it to freeze generation at any time for an arbitrary duration. 
