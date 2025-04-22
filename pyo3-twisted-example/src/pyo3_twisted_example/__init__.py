import gc
import logging
from contextvars import ContextVar
from typing import cast

from twisted.internet import reactor as _reactor
from twisted.internet.defer import ensureDeferred
from twisted.internet.interfaces import IReactorCore, IReactorTime

from pyo3_twisted_example._core import rusty_early_panic, rusty_panic, rusty_sleep, RustPanic

logging.basicConfig(level=logging.DEBUG)


class Reactor(IReactorCore, IReactorTime):
    pass


reactor = cast(Reactor, _reactor)
logger = logging.getLogger(__name__)
var: ContextVar[int] = ContextVar("var")


async def task(n):
    logger.info(f"before sleep {n}")
    var.set(n)
    await rusty_sleep(reactor, n)
    logger.info(f"after sleep {n}")
    logger.info(f"context var is {var.get()}")

async def panic():
    logger.info("before panic")
    try:
        await rusty_panic(reactor)
    except RustPanic:
        logger.exception("caught rust panic")
    logger.info("after panic")

async def early_panic():
    logger.info("before early panic")
    try:
        await rusty_panic(reactor)
    except RustPanic:
        logger.exception("caught rust panic")
    logger.info("after early panic")


def def_task(n):
    return ensureDeferred(task(n)).addTimeout(1.5, reactor)


def gc_now():
    gc.collect()


def main():
    reactor.callWhenRunning(def_task, 1)
    reactor.callWhenRunning(def_task, 2) # This task will timeout
    reactor.callWhenRunning(lambda: ensureDeferred(panic()))
    reactor.callWhenRunning(lambda: ensureDeferred(early_panic()))
    # GC after 2 seconds, so that we see the deferred errors
    reactor.callLater(2, gc_now)
    reactor.run()


if __name__ == "__main__":
    main()
