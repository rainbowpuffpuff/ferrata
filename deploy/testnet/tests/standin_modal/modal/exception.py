"""The stand-in's exceptions, named as Modal's client names them."""


class Error(Exception):
    pass


class RemoteError(Error):
    pass


class OutputExpiredError(Error):
    pass


class FunctionTimeoutError(Error):
    pass
