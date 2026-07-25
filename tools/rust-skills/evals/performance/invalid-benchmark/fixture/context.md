# Fixture

An optimized CRC implementation reports 40% lower time than the readable
reference. The benchmark constructs the same constant 12-byte input every
iteration, ignores the return value, uses one workload size, and includes no
correctness comparison between implementations. The patch deletes the reference
implementation and adds a lookup table that increases the binary by 64 KiB.
