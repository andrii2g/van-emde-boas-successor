# Algorithm and invariants

The public set supports U in {16, 256, 65536}; internal nodes descend through
square-root universes to U=2. Each non-base universe has an even bit width,
so its split is exact and uses integer shifts/masks, never floating point.

For `b = log2(U) / 2`:

```text
high(x)    = x >> b             cluster number
low(x)     = x & ((1 << b)-1)   local key
index(h,l) = (h << b) | l       reconstructed key
```

At U=65536 there are 256 clusters, each with local universe 256, plus one
summary with universe 256. A summary stores cluster numbers, not root keys.
Both kinds of node use the same implementation. All are constructed eagerly.

## Representation

Every node has local `min`/`max`, its local universe, a precomputed split,
an optional boxed summary, and a boxed slice of cluster nodes. U=2 nodes have
no summary or clusters. `None` represents an empty extremum; no integer key
is reserved as a sentinel. The set also maintains its root cardinality.

For a node larger than U=2:

1. The node is empty exactly when both extrema are absent.
2. The minimum is stored only at the node, outside its clusters.
3. Every clustered key reconstructs to a key strictly greater than that minimum.
4. Summary membership is exactly the set of nonempty cluster indices.
5. The maximum equals the largest clustered key, or the minimum for a singleton.
6. Every child independently satisfies these invariants.

The maximum is cached and may also occur in a cluster. At U=2 the two extrema
encode all four possible subsets directly. Validation additionally checks
extrema bounds and root cardinality. Error paths identify summary/cluster
positions only on failure, avoiding diagnostic string allocation on success.

## Queries

Membership first checks extrema. Empty/base nodes terminate immediately;
otherwise it descends into exactly `cluster[high(x)]` with `low(x)`.

Successor is strict. If x precedes the cached minimum, return it; if x is at
least the maximum, there is no answer. Otherwise inspect the current cluster:
when its maximum exceeds low(x), recurse there. If it does not, recursively
find the successor of high(x) in the summary and read that cluster's cached
minimum. Reconstruct the answer with `index`.

Predecessor is symmetric, with an additional fallback to the node's external
minimum when there is no earlier nonempty cluster. No query needs to probe
clusters sequentially.

## Insertion

The public method validates bounds and performs a membership precheck. This
preserves set semantics on duplicates before the unchecked internal update.
An empty node receives both extrema in constant time. If the new key precedes
the minimum, swap it with the minimum, and insert the displaced old minimum
into the recursive structure. Insert the cluster index into the summary only
when that cluster was empty. Then insert the local key and update the maximum.

Although a summary and cluster update can both occur, the latter inserts into
an empty node and terminates in constant time in that case.

## Deletion

The public method checks membership so absent-key deletion is a no-op. A
singleton becomes empty; a two-element U=2 node keeps the other key. Removing
the external minimum promotes the smallest clustered key, found via summary
minimum and cluster minimum, then removes that promoted key from its cluster.
If a cluster becomes empty, remove its index from the summary. If necessary,
rebuild the maximum from summary maximum and cluster maximum, falling back
to the external minimum for a singleton.

When both recursive updates occur, the cluster being emptied was a singleton,
so its deletion was constant-time.

## Bounds

At most one nonconstant recursive branch is needed at a level:

```text
T(U) = T(sqrt(U)) + O(1) = O(log log U)
```

The safe public update wrappers add a membership traversal of the same bound.
Cached extrema and length take O(1). Eager construction/storage are O(U).
The allocated-node recurrence is:

```text
N(2) = 1
N(U) = 1 + (sqrt(U) + 1) * N(sqrt(U))
```

The extra child is the summary. This yields 21, 358, and 92,007 nodes for the
three supported universes. Logical nodes differ from allocator calls: cluster
nodes live inline in a single boxed slice per parent; each non-base parent
also owns one separately boxed summary. Operations do not allocate more nodes.

## SVG query paths

The renderer records the same query decisions in a separate traversal, including
cluster-extremum reads that do not recurse. Keeping that traversal separate
avoids path/string collection in timed queries. Its answers are checked against
the core queries for every query and every subset of universe 16. It never
calls the instrumented public API. Node IDs and layout order are deterministic.
Depth/card limits and hidden-empty branches affect the picture, not the set.
