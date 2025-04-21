from pyo3_twisted_example._core import rusty_panic, rusty_sleep
from typing import cast
from contextvars import ContextVar
from twisted.internet import reactor as _reactor
from twisted.internet.interfaces import IReactorCore, IReactorTime
from twisted.internet.defer import ensureDeferred
import gc

class OurReactor(IReactorCore, IReactorTime):
    pass

reactor = cast(OurReactor, _reactor)

var = ContextVar("var")

async def task(n):
    print("before sleep %s" % n)
    var.set("hello %s" % n)
    await rusty_sleep(reactor, n)
    print("after sleep %s" %n)
    print(var.get())
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
