# Maintenance request

From: a maintainer To: project contributors Subject: The model object has been quietly collecting everyone's jobs can we undo that? Reposting from the contributors' channel with more detail, because I'd like this fixed before the next release. While updating the getting-started example I hit something I can't unsee. The input/dataset plumbing should live with the input utilities where the rest of the package can share it, the sampling and batched-scaling arithmetic belongs with the small math helpers, the rank metrics belong in the evaluation module (importing the model for numbers it could compute itself is backwards), and session ownership belongs with the session helpers not as a forwarder, but actually owning it.

I first ran into this while working around `precision_at_k` in `tensorrec/eval.py`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
