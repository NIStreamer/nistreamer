from nistreamer_backend import StreamerWrap
from .channel import AOChanProxy, DOChanProxy
from .utils import reset_dev
from typing import Union


class BaseCardProxy:

    def __init__(
            self,
            _streamer: StreamerWrap,
            max_name: str,
            nickname=None
    ):
        self._streamer = _streamer
        self.max_name = max_name
        self._nickname = nickname

        self._chans = dict()

    def __getitem__(self, item):
        if item in self._chans:
            return self._chans[item]
        else:
            raise KeyError(f'There is no channel "{item}"')

    # # ToDo: implement to be able to use .keys(), .values(), and .items() to see all channels reserved
    # def __len__(self):
    #     pass
    #
    # def __iter__(self):
    #     pass

    def __repr__(self):
        return (
            f'{self.max_name}\n'
            f'\n'
            f'Channels: {list(self._chans.keys())}\n'
            f'\n'
            f'Hardware settings:\n'
            f'\tSample rate: {self.samp_rate:,} Sa/s\n'
            f'\n'
            f'\tStart trigger: \n'
            f'\t\t in: {self.start_trig_in}\n'
            f'\t\tout: {self.start_trig_out}\n'
            f'\tSample clock:\n'
            f'\t\t in: {self.samp_clk_in}\n'
            f'\t\tout: {self.samp_clk_out}\n'
            f'\t10 MHz reference clock: \n'
            f'\t\t in: {self.ref_clk_in}\n'
            f'\t\tout: see NIStreamer.ref_clk_provider setting\n'
            f'\n'
            f'\tMin buffer write timeout: {self.min_bufwrite_timeout} sec'
        )

    @property
    def nickname(self):
        return self._nickname if self._nickname is not None else self.max_name

    # region Hardware settings
    @property
    def samp_rate(self) -> float:
        return self._streamer.dev_get_samp_rate(name=self.max_name)

    # - Sync settings:
    @property
    def start_trig_in(self) -> Union[str, None]:
        return self._streamer.dev_get_start_trig_in(name=self.max_name)
    @start_trig_in.setter
    def start_trig_in(self, term: Union[str, None]):
        self._streamer.dev_set_start_trig_in(name=self.max_name, term=term)

    @property
    def start_trig_out(self) -> Union[str, None]:
        return self._streamer.dev_get_start_trig_out(name=self.max_name)
    @start_trig_out.setter
    def start_trig_out(self, term: Union[str, None]):
        self._streamer.dev_set_start_trig_out(name=self.max_name, term=term)

    @property
    def samp_clk_in(self) -> Union[str, None]:
        return self._streamer.dev_get_samp_clk_in(name=self.max_name)
    @samp_clk_in.setter
    def samp_clk_in(self, term: Union[str, None]):
        self._streamer.dev_set_samp_clk_in(name=self.max_name, term=term)

    @property
    def samp_clk_out(self) -> Union[str, None]:
        return self._streamer.dev_get_samp_clk_out(name=self.max_name)
    @samp_clk_out.setter
    def samp_clk_out(self, term: Union[str, None]):
        self._streamer.dev_set_samp_clk_out(name=self.max_name, term=term)

    @property
    def ref_clk_in(self) -> Union[str, None]:
        return self._streamer.dev_get_ref_clk_in(name=self.max_name)
    @ref_clk_in.setter
    def ref_clk_in(self, term: Union[str, None]):
        self._streamer.dev_set_ref_clk_in(name=self.max_name, term=term)

    # - Buffer write settings:
    @property
    def min_bufwrite_timeout(self) -> Union[float, None]:
        return self._streamer.dev_get_min_bufwrite_timeout(name=self.max_name)
    @min_bufwrite_timeout.setter
    def min_bufwrite_timeout(self, min_timeout: Union[float, None]):
        self._streamer.dev_set_min_bufwrite_timeout(name=self.max_name, min_timeout=min_timeout)
    # endregion

    def clear_edit_cache(self):
        self._streamer.dev_clear_edit_cache(name=self.max_name)

    def reset(self):
        reset_dev(name=self.max_name)

    def last_instr_end_time(self):
        return self._streamer.dev_last_instr_end_time(name=self.max_name)


class AOCardProxy(BaseCardProxy):

    def __repr__(self):
        return 'AO card ' + super().__repr__()

    def add_chan(
            self,
            chan_idx: int,
            dflt_val: float = 0.0,
            rst_val: float = 0.0,
            nickname: str = None,
            proxy_class=AOChanProxy
    ):
        # Raw Rust NIStreamer call
        self._streamer.add_ao_chan(
            dev_name=self.max_name,
            chan_idx=chan_idx,
            dflt_val=dflt_val,
            rst_val=rst_val
        )
        # Instantiate proxy object
        chan_proxy = proxy_class(
            _streamer=self._streamer,
            _card_max_name=self.max_name,
            chan_idx=chan_idx,
            nickname=nickname
        )
        self._chans[chan_proxy.chan_name] = chan_proxy
        return chan_proxy


class DOCardProxy(BaseCardProxy):

    def __repr__(self):
        return 'DO card ' + super().__repr__() + f'\n\n\tConst fns only: {self.const_fns_only}'

    def add_chan(
            self,
            port_idx: int,
            line_idx: int,
            dflt_val: bool = False,
            rst_val: bool = False,
            nickname: str = None,
            proxy_class=DOChanProxy
    ):
        # Raw Rust NIStreamer call
        self._streamer.add_do_chan(
            dev_name=self.max_name,
            port_idx=port_idx,
            line_idx=line_idx,
            dflt_val=dflt_val,
            rst_val=rst_val,
        )
        # Instantiate proxy object
        chan_proxy = proxy_class(
            _streamer=self._streamer,
            _card_max_name=self.max_name,
            port_idx=port_idx,
            line_idx=line_idx,
            nickname=nickname
        )
        self._chans[chan_proxy.chan_name] = chan_proxy
        return chan_proxy

    @property
    def const_fns_only(self):
        return self._streamer.dodev_get_const_fns_only(name=self.max_name)

    @const_fns_only.setter
    def const_fns_only(self, val):
        self._streamer.dodev_set_const_fns_only(name=self.max_name, val=val)
