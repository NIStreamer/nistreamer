"""Miscellaneous helper tools."""

from ._nistreamer import connect_terms as _connect_terms
from ._nistreamer import disconnect_terms as _disconnect_terms
from ._nistreamer import reset_dev as _reset_dev
import numpy as np
from typing import Union, Optional
# Import plotly
PLOTLY_INSTALLED = False
try:
    import plotly.graph_objects as go
    from plotly.subplots import make_subplots
    PLOTLY_INSTALLED = True
except ImportError:
    print(
        'Warning! Plotly package is not installed. You can still use the streamer, '
        'but plotting functionality will not be available.\n'
        'To install, run `pip install plotly` in your Python environment'
    )


# region NI DAQmx functions
def connect_terms(src: str, dest: str):
    """Create a static connection between terminals.

    *Hint:* you can find the list of available terminals and signals as well
    as permitted routes for each card using NI MAX app. Click the specific
    card entry in the device tree on the left, then hit the "Device Routes"
    tab on the bottom of the window.

    **CAUTION!** Static conntections are independent of any NI tasks and will
    persist until explicily undone, involved cards are reset, or the full system
    is power-cycled. If left behind, such static connections can lead to physical
    line double-driving, very confusing sync issues, and even hardware damage.

    Args:
        src: full source terminal or signal name
        dest: full destination terminal name

    Raises:
        ValueError: if any terminal name is invalid or if the connection cannot be established.

    Examples:
        >>> connect_terms(src='/Dev1/PFI0', dest='/Dev1/PXI_Trig0')
        >>> connect_terms(src='/Dev2/10MHzRefClock', dest='/Dev2/PFI0')

    See Also:
         Use :meth:`disconnect_terms` or :meth:`reset_dev` to undo static connections.
    """
    return _connect_terms(src=src, dest=dest)


def disconnect_terms(src: str, dest: str):
    """Undo static connection.

    Args:
        src: full source terminal or signal name
        dest: full destination terminal name

    Raises:
        ValueError: if any terminal name is invalid.
    """
    return _disconnect_terms(src=src, dest=dest)


def share_10mhz_ref(dev: str, term: str):
    """Statically export 10 MHz reference clock signal.

    **CAUTION!** Static conntections are independent of any NI tasks and will
    persist until explicily undone, involved cards are reset, or the full system
    is power-cycled. If left behind, such static connections can lead to physical
    line double-driving, very confusing sync issues, and even hardware damage.

    Args:
        dev: device name
        term: terminal name

    Raises:
        ValueError: if parameters are invalid

    Examples:
        >>> share_10mhz_ref(dev='Dev1', term='PFI0')

    See Also:
        Consider using a safer way of 10 MHz reference export by setting
        :meth:`~nistreamer.streamer.NIStreamer.ref_clk_provider` property.
        This will automatically undo export when run is finished.

        If still choosing manual approach, use :meth:`unshare_10mhz_ref`
        or :meth:`reset_dev` to undo this export afterwards.
    """
    connect_terms(
        src=f'/{dev}/10MHzRefClock',
        dest=f'/{dev}/{term}'
    )


def unshare_10mhz_ref(dev: str, term: str):
    """Undo static export of 10 MHz reference clock signal.

    Args:
        dev: device name
        term: terminal name

    Raises:
        ValueError: if parameters are invalid
    """
    disconnect_terms(
        src=f'/{dev}/10MHzRefClock',
        dest=f'/{dev}/{term}'
    )


def reset_dev(name: str):
    """Perform hardware reset.

    Args:
        name: device name as shown in NI MAX
    """
    return _reset_dev(name=name)
# endregion


# region iplot
class RendOption:
    """Enum-like collection of select Plotly renderer options.

    See `Plotly docs <https://plotly.com/python/renderers/>`_
    for the full list of available renderers.
    """
    browser = 'browser'
    notebook = 'notebook'
    svg = 'svg'
    png = 'png'
    jpeg = 'jpeg'


def iplot(chan_list,
          start_time: Union[float, None] = None,
          end_time: Union[float, None] = None,
          nsamps: Optional[int] = 1000,
          renderer: Optional[str] = 'browser',
          row_height: Union[float, None] = None):
    """Plot signals for a list of channels.

    Values are computed for a grid of ``nsamps`` time points uniformly
    distributed over the closed interval ``[start_time, end_time]``.
    Note that sequence has to be freshly compiled to plot.

    Args:
        chan_list: list of channel proxy instances to plot (trace order will follow the list order)
        start_time: window start time. If ``None``, zero time is used
        end_time: window end time. If ``None``, compiled sequence end time is used
        nsamps: number of time points to evaluate for each channel
        renderer: Plotly renderer to use. A few options are collected in :class:`RendOption`
        row_height: channel sub-plot height

    Raises:
        ImportError: if ``plotly`` is not installed
        ValueError: if sequence is not freshly compiled or any parameters are invalid

    Notes:
        You may need to select a sufficiently large ``nsamps`` and a sufficiently
        narrow time window to see the true waveform shape that the actual stream
        would produce when sampling at the hardware clock rate. Otherwise, very
        narrow pulses may be missed and periodic waveforms may appear distorted
        due to undersampling.
    """
    if not PLOTLY_INSTALLED:
        raise ImportError('Plotly package is not installed. Run `pip install plotly` to get it.')

    # Sanity checks:
    if len(chan_list) == 0:
        raise ValueError("Channel list is empty")
    streamer_wrap = chan_list[0]._streamer  # FixMe: this is a dirty hack. Consider making this function a method of NIStreamer class to get clear access to streamer_wrap
    streamer_wrap.validate_compile_cache()

    # Process window start/end times
    total_run_time = streamer_wrap.shortest_dev_run_time()
    if start_time is not None:
        if start_time > total_run_time:
            raise ValueError(f"Requsted start_time={start_time} exceeds total run time {total_run_time}")
    else:
        start_time = 0.0

    if end_time is not None:
        if end_time > total_run_time:
            raise ValueError(f"Requsted end_time={end_time} exceeds total run time {total_run_time}")
    else:
        end_time = total_run_time

    if start_time > end_time:
        raise ValueError(f"Requested start_time={start_time} exceeds end_time={end_time}")

    t_arr = np.linspace(start_time, end_time, nsamps)

    chan_num = len(chan_list)
    nsamps = int(nsamps)
    fig = make_subplots(
        rows=len(chan_list),
        cols=1,
        x_title='Time [s]',
        # shared_xaxes=True,  # Using this option locks X-axes,
                              # but also hides X-axis ticks for all plots except the bottom one
    )
    fig.update_xaxes(matches='x')  # Using this option locks X-axes and also leaves ticks

    if row_height is not None:
        fig.update_layout(height=1.1 * row_height * chan_num)
    else:
        # Row height is not provided - use auto-height and fit everything into the standard frame height.
        #
        # Exception - the case of many channels:
        #   - switch off auto and set fixed row height, to make frame extend downwards as much as needed
        if chan_num > 4:
            fig.update_layout(height=1.1 * 200 * chan_num)

    for idx, chan in enumerate(chan_list):
        signal_arr = chan.calc_signal(start_time=start_time, end_time=end_time, nsamps=nsamps)
        fig.add_trace(
            go.Scatter(x=t_arr, y=signal_arr, name=chan.nickname),
            row=idx + 1, col=1
        )

    fig.show(renderer=renderer)
# endregion
