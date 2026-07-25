# Fixture

A stateful 4:1 FIR decimator receives chunks with `start_sample: u64`. When the
next chunk starts later than expected, the current proposal logs a warning and
continues with the existing delay line and decimation phase. Output records do
not carry an epoch, loss extent, or input-to-output position mapping.
