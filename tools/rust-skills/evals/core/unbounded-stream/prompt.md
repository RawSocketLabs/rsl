# Task

Review and redesign this long-running sample pipeline so a fast producer cannot
grow memory without bound. The application prioritizes current streaming data,
so overload should drop the oldest queued block. Keep the base implementation
synchronous and make shutdown explicit.
