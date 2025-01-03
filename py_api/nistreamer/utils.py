import numpy as np
from nistreamer_backend import connect_terms as raw_connect_terms
from nistreamer_backend import disconnect_terms as raw_disconnect_terms
from nistreamer_backend import reset_dev as raw_reset_dev
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
    """Statically (independently of any NI task) connect terminals

    :param src:
    :param dest:
    :return:
    """
    return raw_connect_terms(src=src, dest=dest)


def disconnect_terms(src: str, dest: str):
    return raw_disconnect_terms(src=src, dest=dest)


def share_10mhz_ref(dev: str, term: str):
    connect_terms(
        src=f'/{dev}/10MHzRefClock',
        dest=f'/{dev}/{term}'
    )


def unshare_10mhz_ref(dev: str, term: str):
    disconnect_terms(
        src=f'/{dev}/10MHzRefClock',
        dest=f'/{dev}/{term}'
    )


def reset_dev(name: str):
    return raw_reset_dev(name=name)
# endregion


# region iplot
class RendOption:

    # Available renderers (from https://plotly.com/python/renderers/):
    #         ['plotly_mimetype', 'jupyterlab', 'nteract', 'vscode',
    #          'notebook', 'notebook_connected', 'kaggle', 'azure', 'colab',
    #          'cocalc', 'databricks', 'json', 'png', 'jpeg', 'jpg', 'svg',
    #          'pdf', 'browser', 'firefox', 'chrome', 'chromium', 'iframe',
    #          'iframe_connected', 'sphinx_gallery', 'sphinx_gallery_png']

    browser = 'browser'
    notebook = 'notebook'


def iplot(chan_list, start_time=None, end_time=None, nsamps=1000, renderer='browser', row_height=None):
    if not PLOTLY_INSTALLED:
        raise ImportError('Plotly package is not installed. Run `pip install plotly` to get it.')

    # FixMe: this is a dirty hack.
    #  Consider making this function a method of NIStreamer class to get clear access to streamer_wrap
    streamer_wrap = chan_list[0]._streamer

    streamer_wrap.validate_compile_cache()

    total_run_time = streamer_wrap.total_run_time()
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

    # ToDo:
    #   `src_pwr` (`slow_ao_card.ao0`) did not receive any instructions, resulting in this error
    #   PanicException: Attempting to calculate signal on not-compiled channel ao0
    #   Try checking edit cache with `is_edited`

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
