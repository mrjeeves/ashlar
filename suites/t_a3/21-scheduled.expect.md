## Correct reading

`every = "10m"` declares a recurring schedule: the part's `run` function
executes every 10 minutes, logging "sweeping".

## Must state

- `every = "10m"` expresses a recurring schedule attached to the part.
- `"10m"` reads as a duration of ten minutes (the schedule's interval).
- `run` is a zero-parameter function that logs `"sweeping"`; it is what
  the schedule executes.
- `sweep` is a part grouping the schedule (`every`) with its action
  (`run`).
