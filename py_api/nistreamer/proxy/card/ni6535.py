from nistreamer.card import BaseCardProxy, DOCardProxy
from nistreamer.channel import DOChanProxy


class NI6535(DOCardProxy):
    def __repr__(self):
        return super().__repr__() + '\n\n\tCard model: NI6535'

    def add_chan(
            self,
            chan_idx: int,
            dflt_val: bool = False,
            rst_val: bool = False,
            nickname: str = None,
            proxy_class=DOChanProxy
    ):
        return super().add_chan(
            port_idx=chan_idx // 8,
            line_idx=chan_idx % 8,
            dflt_val=dflt_val,
            rst_val=rst_val,
            nickname=nickname,
            proxy_class=proxy_class
        )
