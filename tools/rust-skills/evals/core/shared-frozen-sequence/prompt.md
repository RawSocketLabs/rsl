# Task

Profiling identified allocation and element-copy cost while cloning large
`FilterPlan` values into several long-lived worker configurations. Coefficients
are frozen after construction, all workers intentionally use identical values,
and plans cross thread boundaries. Improve the ownership design, preserve
behavior, and distinguish validation you run from further measurements you
recommend.
