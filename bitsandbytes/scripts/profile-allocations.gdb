# Linux x86_64 / system allocator, debug example build. Not a wall-clock benchmark.
# cargo build -p bitsandbytes --example bitbuf_bounded
# gdb -q -batch -x bitsandbytes/scripts/profile-allocations.gdb --args target/debug/examples/bitbuf_bounded --profile
set pagination off
set breakpoint pending on
set confirm off
set debuginfod enabled off
python
import gdb

phase = None
metrics = None
seen = set()
expected = {1: (0, 0), 2: (0, 0), 3: (0, 0), 4: (0, 0),
            5: (1, 4096), 6: (1, 3), 7: (0, 0), 8: (1, 64), 9: (0, 0)}

class Marker(gdb.Breakpoint):
    def __init__(self, name, starting):
        super().__init__("bitbuf_bounded::allocation_" + name, internal=True)
        self.starting = starting

    def stop(self):
        global phase, metrics
        value = int(gdb.selected_frame().read_var("phase"))
        if self.starting:
            assert phase is None
            phase = value
            metrics = dict(allocs=0, bytes=0, reallocs=0, moved=0)
        else:
            assert phase == value
            print("phase %d: %s" % (value, metrics))
            calls, size = expected[value]
            valid = (metrics["allocs"], metrics["bytes"]) == (calls, size)
            valid = valid and metrics["reallocs"] == 0
            if value == 7:
                # Two retained bytes compacted, then two freshly pushed bytes copied.
                valid = valid and metrics["moved"] == 4
            if value == 9:
                # Characterization: rustc 1.98, Linux x86_64, debug/system allocator.
                # Only the appended byte moves; copying the 56 retained bytes is avoidable.
                # Recalibrate for changed compiler lowering, not changed buffer policy.
                valid = valid and metrics["moved"] == 1
            if not valid:
                gdb.execute("quit 1")
            seen.add(value)
            phase = None
        return False

class Allocation(gdb.Breakpoint):
    def __init__(self, name):
        super().__init__(name, internal=True)
        self.operation = name

    def stop(self):
        if phase is not None:
            first = int(gdb.parse_and_eval("$rdi"))
            if self.operation == "memmove":
                metrics["moved"] += int(gdb.parse_and_eval("$rdx"))
            elif self.operation == "realloc":
                metrics["reallocs"] += 1
            else:
                metrics["allocs"] += 1
                size = first
                if self.operation == "calloc":
                    size *= int(gdb.parse_and_eval("$rsi"))
                metrics["bytes"] += size
        return False

Marker("start", True)
Marker("end", False)
for name in ("malloc", "calloc", "realloc", "memmove"):
    Allocation(name)
end
run
python
if seen != set(expected):
    print("missing profiling phases:", set(expected) - seen)
    gdb.execute("quit 1")
end
