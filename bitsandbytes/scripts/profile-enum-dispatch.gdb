# Linux x86_64 / system allocator. Allocation accounting, not a timing benchmark.
# cargo bench -p bitsandbytes --bench bitstream_bench --no-run
# gdb -q -batch -x bitsandbytes/scripts/profile-enum-dispatch.gdb --args <bench executable>
# Set BNB_PROFILE_BASELINE=1 when measuring the pre-diagnostics executable.
set pagination off
set breakpoint pending on
set confirm off
set debuginfod enabled off
set environment BNB_DISPATCH_PROBE 1
python
import gdb
import os

phase = 0
metrics = None
seen = []
baseline = os.environ.get("BNB_PROFILE_BASELINE") == "1"

class End(gdb.FinishBreakpoint):
    def stop(self):
        global metrics
        print("phase %d: %s" % (phase, metrics))
        seen.append(metrics)
        metrics = None
        return False

class Start(gdb.Breakpoint):
    def stop(self):
        global metrics, phase
        phase += 1
        metrics = dict(allocs=0, bytes=0, reallocs=0)
        End(internal=True)
        return False

class Allocation(gdb.Breakpoint):
    def __init__(self, name):
        super().__init__(name, internal=True)
        self.operation = name

    def stop(self):
        if metrics is not None:
            if self.operation == "realloc":
                metrics["reallocs"] += 1
            else:
                metrics["allocs"] += 1
                size = int(gdb.parse_and_eval("$rdi"))
                if self.operation == "calloc":
                    size *= int(gdb.parse_and_eval("$rsi"))
                metrics["bytes"] += size
        return False

Start("bitstream_bench::probe_integer", internal=True)
Start("bitstream_bench::probe_bytes", internal=True)
for name in ("malloc", "calloc", "realloc"):
    Allocation(name)
end
run
python
# Each phase executes 1000 attempts: numeric success/failure, byte success/failure.
expected_calls = [0, 1000, 0, 1000 if baseline else 2000]
valid = len(seen) == 4 and [m["allocs"] for m in seen] == expected_calls
valid = valid and all(m["reallocs"] == 0 for m in seen)
if valid:
    valid = seen[0]["bytes"] == seen[2]["bytes"] == 0
    if not baseline:
        # The extra allocation contains just the four-byte discriminator, not a frame.
        valid = valid and seen[3]["bytes"] - seen[1]["bytes"] == 4000
if not valid:
    print("allocation contract failed:", seen)
    gdb.execute("quit 1")
end
