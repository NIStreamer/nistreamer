"""Streamer module.

Contains the main ``NIStreamer`` class, as well as ``StreamHandle`` class
for stream control in ``with`` context.
"""

from nistreamer_backend import StreamerWrap as _StreamerWrap
from .card import BaseCardProxy, AOCardProxy, DOCardProxy
from typing import Optional, Literal, Union, Tuple, Type


class NIStreamer:
    """Represents the whole streamer
    """

    def __init__(self):
        """Creates a new empty instance."""
        self._streamer = _StreamerWrap()
        self._ao_cards = dict()
        self._do_cards = dict()

    def __getitem__(self, item):
        if item in self._ao_cards.keys():
            return self._ao_cards[item]
        elif item in self._do_cards.keys():
            return self._do_cards[item]
        else:
            raise KeyError(f'There is no card with max_name "{item}" registered')

    def __repr__(self):
        return (
            f'NIStreamer instance\n'
            f'\n'
            f'AO cards: {list(self._ao_cards.keys())}\n'
            f'DO cards: {list(self._do_cards.keys())}\n'
            f'\n'
            f'Hardware settings:\n'
            f'\tCalc/write chunk size: {self.chunksize_ms} ms\n'
            f'\t   10MHz ref provider: {self.ref_clk_provider}\n'
            f'\t     Starts-last card: {self.starts_last}'
        )

    def _add_card(
            self,
            card_type: Literal['AO', 'DO'],
            max_name: str,
            samp_rate: float,
            proxy_class: Type[BaseCardProxy],
            nickname: Optional[str] = None,
     ) -> BaseCardProxy:
        """Base for ``add_card`` methods."""

        if card_type == 'AO':
            streamer_method = _StreamerWrap.add_ao_dev
            target_dict = self._ao_cards
        elif card_type == 'DO':
            streamer_method = _StreamerWrap.add_do_dev
            target_dict = self._do_cards
        else:
            raise ValueError(f'Invalid card type "{card_type}". Valid type strings are "AO" and "DO"')

        # Call to NIStreamer struct in the compiled Rust backend
        streamer_method(
            self._streamer,
            name=max_name,
            samp_rate=samp_rate
        )
        # Proxy object
        proxy = proxy_class(
            _streamer=self._streamer,
            max_name=max_name,
            nickname=nickname
        )
        target_dict[max_name] = proxy
        return proxy

    def add_ao_card(
            self,
            max_name: str,
            samp_rate: float,
            nickname: Optional[str] = None,
            proxy_class: Optional[Type[BaseCardProxy]] = AOCardProxy
    ):
        """Add an analog output card.

        Args:
            max_name: name of the card as shown in NI MAX
            samp_rate: sample rate in Hz
            nickname: human-readable name (e.g. "Fast AO card"; used for visualizations)
            proxy_class: custom subclass of ``BaseCardProxy`` to use for the device proxy.

        Returns:
            ``proxy_class`` instance representing this card.

        Raises:
            KeyError: if a card with the same name already exists.
        """
        return self._add_card(
            card_type='AO',
            max_name=max_name,
            samp_rate=samp_rate,
            nickname=nickname,
            proxy_class=proxy_class
        )

    def add_do_card(
            self,
            max_name: str,
            samp_rate: float,
            nickname: Optional[str] = None,
            proxy_class: Optional[Type[BaseCardProxy]] = DOCardProxy
    ):
        """Add a digital output card.

        Args:
            max_name: name of the card as shown in NI MAX
            samp_rate: sample rate in Hz
            nickname: human-readable name (e.g. "Fast DO card"; used for visualizations)
            proxy_class: custom subclass of ``BaseCardProxy`` to use for the device proxy.

        Returns:
            ``proxy_class`` instance representing this card.

        Raises:
            KeyError: if a card with the same name already exists.
        """
        return self._add_card(
            card_type='DO',
            max_name=max_name,
            samp_rate=samp_rate,
            nickname=nickname,
            proxy_class=proxy_class
        )

    @property
    def chunksize_ms(self) -> float:
        """Streaming chunk size in milliseconds.

        Chunk size is the main unit of streaming - all signal samples within
        a single chunk are computed, stored in memory, and transferred to the
        hardware in one operation together, at the same time as the previous
        chunk is playing.

        Optimal chunk size depends on the tradeoff. Larger size reduces the
        risk of buffer underflow at the cost of increased overhead - memory
        allocation and the initial chunk compute will take longer.
        The default value is 150 ms.

        Notes:
            If chunk size exceeds the total sequence duration, no streaming
            actually happens - all samples are computed before generation
            starts. This could be used to eliminate the risk of underflows
            if sequence is sufficiently short. However, in-stream looping
            feature will be disabled, and trying to pre-sample a long
            waveform can overfill RAM.
        """
        return self._streamer.get_chunksize_ms()

    @chunksize_ms.setter
    def chunksize_ms(self, val: float):
        self._streamer.set_chunksize_ms(val=val)

    @property
    def starts_last(self) -> Union[str, None]:
        """Specifies which card starts last.

        Format:
            * ``dev_name: str`` - this card waits for all others to start first;
            * ``None`` (default) - each threads starts its task independently.

        Typically, this is needed when start trigger or shared sample clock
        are used for hardware synchronisation. The card-provider of the signal
        should call ``ni_task.start()`` last, otherwise some "consumer" cards
        could start later and miss the signal.
        """
        return self._streamer.get_starts_last()

    @starts_last.setter
    def starts_last(self, name: Union[str, None]):
        self._streamer.set_starts_last(name=name)

    @property
    def ref_clk_provider(self) -> Union[Tuple[str, str], None]:
        """Specifies which card exports its 10 MHz reference clock signal during the run.

        Format:
            * ``(card_name: str, term_name: str)`` - card `card_name` exports to terminal `term_name`
            * ``None`` - no card exports

        Notes:
            (1) NIStreamer uses *run-based static* reference clock export -
            signal is statically exported during ``init_stream()`` and then
            automatically undone during ``close_stream()`` calls.

            This export is *static* instead of being tied to NI tasks.
            As a result, any card supporting 10MHz export can serve as the
            provider. It does not have to be active (get some instructions)
            or even be registered in the streamer.

            (2) If this mechanism is not sufficient, you can manually do a
            static export from any card by calling ``utils.share_10mhz_ref()``.
            However, such export will not be undone automatically - you should
            manually call ``utils.unshare_10mhz_ref()`` or ``utils.reset_dev()``.

            Forgetting to undo manual export is very easy and dangerous -
            the exporter card will continue silently feeding the signal until
            the system is power-cycled. That can lead to physical
            double-driving and very confusing mis-triggering or mis-clocking
            errors. So only choose the manual export over the automatic method
            (1) if absolutely necessary.
        """
        return self._streamer.get_ref_clk_provider()

    @ref_clk_provider.setter
    def ref_clk_provider(self, dev_and_term: Union[Tuple[str, str], None]):
        self._streamer.set_ref_clk_provider(provider=dev_and_term)

    def got_instructions(self) -> bool:
        """Returns ``True`` if there are some instructions in the edit cache, otherwise ``False``."""
        return self._streamer.got_instructions()

    def last_instr_end_time(self) -> Union[float, None]:
        """Returns the last instruction end time or ``None`` if the edit cache is empty."""
        return self._streamer.last_instr_end_time()

    def compile(self, stop_time: Optional[float] = None) -> float:
        """Compiles the full pulse sequence from instructions in the current edit cache.

        Args:
            stop_time: If ``None`` (default), the compiled sequence stops at the last instruction end.
                Specifying a later stop time extends the sequence duration.

        Returns:
            The actual compiled stop time.

        Raises:
            ValueError: if provided ``stop_time`` is below the last instruction end.

        Notes:
            The actual stop times may vary between cards due to clock grid mismatch
            and extra ticks on the final closing edges. The returned value is the
            shortest run time across all cards.

            If explicit ``stop_time`` is provided, the additional time at the
            sequence end will be filled according to the usual rules:

            - Constant values after finite-duration instructions.
            - Continued waveforms after "go-this" instructions.
        """
        return self._streamer.compile(stop_time=stop_time)

    def validate_compile_cache(self):
        """Verifies that compile cache is up-to-date with the current edit cache.

        Returns:
            ``None`` if compile cache is up-to-date with the current edit cache.

        Raises:
            ValueError: If compile cache does not match edit cache
                (typical reason - users forgot to re-compile after adding more instructions).
        """
        self._streamer.validate_compile_cache()

    def init_stream(self):
        """Context-based stream initialization.

        This function should only be used in the ``with`` context and is intended for
        advanced cases. For basic streaming use :meth:`run` instead.

        Examples:

            >>> with my_streamer.init_stream() as stream_handle:
            >>>     stream_handle.launch(instream_reps=10)
            >>>     # Do some other logic while waiting ...
            >>>     stream_handle.wait_until_finished()

        See Also:

            :class:`NIStreamer.StreamHandle`: for more details about stream controls.
        """
        return self._ContextManager(streamer=self._streamer)

    def run(self, nreps: Optional[int] = 1):
        """Runs pulse sequence generation.

        This method will block and only return after all ``nreps`` iterations have been generated.
        But it can be interrupted with ``KeyboardInterrupt`` - generation will stop between repetitions.

        Args:
            nreps: the number of times to play the sequence. Plays once by default.

        Raises:
            ValueError: if stream initialization fails due to invalid settings.
            RuntimeError: if generation fails during streaming (underflow, timeout, overvoltage).

        Notes:

            This method implements basic repeating by stopping and re-starting the stream every time.
            As a result, there is a fluctuating time gap between subsequent repetitions.

            Repeating with no gap and rigid timing between iterations is possible with so-called
            *in-stream looping*. It is only accessible through the context-based interface.
            See :meth:`init_stream` and :class:`NIStreamer.StreamHandle` for more details.
        """
        with self.init_stream() as stream_handle:
            for _ in range(nreps):
                stream_handle.launch(instream_reps=1)
                stream_handle.wait_until_finished()

    def close_stream(self):
        """Closes the stream.

        You typically do not need to use this method since context manager
        will automatically close the stream whenever exiting the context.

        There is a small chance that a rapid succession of ``KeyboardInterrupt``
        signals disrupts ``__exit__()`` logic and prevents automatic stream shutdown.
        But even then, the next call to ``init`` will automatically close it.

        So this method is mostly exposed for completeness.
        """
        return self._streamer.close_stream()

    def add_reset_instr(self, reset_time: Optional[float] = None):
        """Helper method - adds a "go-reset-value" instruction on each channel.

        Args:
            reset_time: the time at which to add the reset instruction
                (the same for all channels). If ``None``, the last instruction
                end time across all channels is determined and used.

        Raises:
            ValueError: if requested ``reset_time`` is below the last
                instruction end time.
        """
        self._streamer.add_reset_instr(reset_time=reset_time)

    def clear_edit_cache(self):
        """Discards all instructions added so far."""
        self._streamer.clear_edit_cache()

    def reset_all(self):
        """Performs hardware reset on all cards that have been added to the streamer."""
        self._streamer.reset_all()

    class _ContextManager:
        """Using explicit context manager class instead of `contextlib.contextmanager` decorator
        because it results in a shorter traceback from exceptions during `__enter__()`.
        This is important - users will see an exception any time `init_stream()` fails due to invalid user settings.
        """

        def __init__(self, streamer: _StreamerWrap):
            self._streamer = streamer

        def __enter__(self):
            # The actual stream initialization call to the back-end:
            self._streamer.init_stream()
            # If the above call returned without exceptions, stream has been successfully initialized.
            # Return the handle for use in the `with` context body:
            return NIStreamer.StreamHandle(streamer=self._streamer)

        def __exit__(self, *args, **kwargs):
            self._streamer.close_stream()

    class StreamHandle:
        def __init__(self, streamer: _StreamerWrap):
            self._streamer = streamer

        def launch(self, instream_reps=1):
            self._streamer.launch(instream_reps=instream_reps)

        def wait_until_finished(self, timeout: Optional[float] = None):
            """There are two modes available depending on `timeout` value.

            "Basic" mode - `timeout = None` (default).
            Blocks until run is finished, but is interruptable with `KeyboardInterrupt`.
            Returns `None` when run is finished.

            "Advanced" mode - `timeout: float` - timeout in seconds.
            Blocks and returns `True` when run is finished or `False` if timeout elapses.
            This mode can be used together with `reps_written_count()` to implement a "progress bar".

            If there is a stream error, this method raises `RuntimeError` in either mode.
            """
            if timeout is None:
                while True:
                    if self._streamer.wait_until_finished(timeout=1.0):
                        break
            else:
                return self._streamer.wait_until_finished(timeout=timeout)

        def request_stop(self):
            """You do not need to use this function in most cases - leaving `with` context
            while stream is still running will automatically request the stop and will wait
            until the stream cleanly finishes before returning.

            This function is only exposed for possible advanced applications where you need
            to break out of in-stream repetition loop and then want to re-launch the stream
            again without spending extra time on redoing initialization."""
            self._streamer.request_stop()

        def reps_written_count(self):
            return self._streamer.reps_written_count()

