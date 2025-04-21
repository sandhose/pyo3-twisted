import gc
from contextvars import ContextVar
from typing import cast

from twisted.internet import reactor as _reactor
from twisted.internet.defer import ensureDeferred
from twisted.internet.interfaces import IReactorCore, IReactorTime

from pyo3_twisted_example._core import rusty_panic, rusty_sleep


class Reactor(IReactorCore, IReactorTime):
    pass

reactor = cast(Reactor, _reactor)

var: ContextVar[int] = ContextVar("var")

async def task(n):
    print(f"before sleep {n}")
    var.set(n)
    await rusty_sleep(reactor, n)
    print(f"after sleep {n}")
    print(f"context var is {var.get()}")
    await rusty_panic(reactor)

def def_task(n):
    return ensureDeferred(task(n)).addTimeout(1.5, reactor)

def gc_now():
    gc.collect()

def main():
    reactor.callWhenRunning(def_task, 1)
    reactor.callWhenRunning(def_task, 2)
    # GC after 3 seconds, so that we see the deferred errors
    reactor.callLater(3, gc_now)
    reactor.run()


if __name__ == "__main__":
    main()
