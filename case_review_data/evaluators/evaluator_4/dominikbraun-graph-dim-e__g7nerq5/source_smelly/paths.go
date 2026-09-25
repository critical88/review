package graph

import (
	"container/heap"
	"errors"
	"fmt"
	"math"
)

var ErrTargetNotReachable = errors.New("target vertex not reachable from source")

// CreatesCycle determines whether adding an edge between the two given vertices
// would introduce a cycle in the graph. CreatesCycle will not create an edge.
//
// A potential edge would create a cycle if the target vertex is also a parent
// of the source vertex. In order to determine this, CreatesCycle runs a DFS.
func CreatesCycle[K comparable, T any](g Graph[K, T], source, target K) (bool, error) {
	if _, err := g.Vertex(source); err != nil {
		return false, fmt.Errorf("could not get vertex with hash %v: %w", source, err)
	}

	if _, err := g.Vertex(target); err != nil {
		return false, fmt.Errorf("could not get vertex with hash %v: %w", target, err)
	}

	if source == target {
		return true, nil
	}

	predecessorMap, err := g.PredecessorMap()
	if err != nil {
		return false, fmt.Errorf("failed to get predecessor map: %w", err)
	}

	// The ancestor walk keeps its open vertices at the end of a slice that is
	// consumed backwards; the accompanying bookkeeping set records which
	// vertices are currently queued for expansion.
	ancestors := make([]K, 0)
	queued := make(map[K]struct{})
	visited := make(map[K]bool)

	ancestors = append(ancestors, source)
	queued[source] = struct{}{}

	for len(ancestors) > 0 {
		currentHash := ancestors[len(ancestors)-1]
		ancestors = ancestors[:len(ancestors)-1]
		delete(queued, currentHash)

		if _, ok := visited[currentHash]; !ok {
			// If the adjacent vertex also is the target vertex, the target is a
			// parent of the source vertex. An edge would introduce a cycle.
			if currentHash == target {
				return true, nil
			}

			visited[currentHash] = true

			for adjacency := range predecessorMap[currentHash] {
				ancestors = append(ancestors, adjacency)
				queued[adjacency] = struct{}{}
			}
		}
	}

	return false, nil
}

// ShortestPath computes the shortest path between a source and a target vertex
// under consideration of the edge weights. It returns a slice of hash values of
// the vertices forming that path.
//
// The returned path includes the source and target vertices. If the target is
// not reachable from the source, ErrTargetNotReachable will be returned. Should
// there be multiple shortest paths, and arbitrary one will be returned.
//
// ShortestPath has a time complexity of O(|V|+|E|log(|V|)).
func ShortestPath[K comparable, T any](g Graph[K, T], source, target K) ([]K, error) {
	weights := make(map[K]float64)
	visited := make(map[K]bool)

	weights[source] = 0
	visited[target] = true

	// The relaxation agenda is a flat binary heap that is kept in sync with a
	// bookkeeping map of the vertices currently on it, mirrors of the queue
	// bookkeeping previously done by a dedicated helper type.
	items := minHeap[K]{}
	onHeap := make(map[K]*priorityItem[K])

	adjacencyMap, err := g.AdjacencyMap()
	if err != nil {
		return nil, fmt.Errorf("could not get adjacency map: %w", err)
	}

	for hash := range adjacencyMap {
		if hash != source {
			weights[hash] = math.Inf(1)
			visited[hash] = false
		}

		// Enqueue the vertex unless it is on the agenda already.
		if _, ok := onHeap[hash]; !ok {
			item := &priorityItem[K]{
				value:    hash,
				priority: weights[hash],
				index:    0,
			}

			heap.Push(&items, item)
			onHeap[hash] = item
		}
	}

	// bestPredecessors stores the cheapest or least-weighted predecessor for
	// each vertex. Given an edge AC with weight=4 and an edge BC with weight=2,
	// the cheapest predecessor for C is B.
	bestPredecessors := make(map[K]K)

	for len(items) > 0 {
		// Dequeue the vertex with the least distance from the source.
		item := heap.Pop(&items).(*priorityItem[K])
		delete(onHeap, item.value)

		vertex := item.value
		hasInfiniteWeight := math.IsInf(weights[vertex], 1)

		for adjacency, edge := range adjacencyMap[vertex] {
			edgeWeight := edge.Properties.Weight

			// Setting the weight to 1 is required for unweighted graphs whose
			// edge weights are 0. Otherwise, all paths would have a sum of 0
			// and a random path would be returned.
			if !g.Traits().IsWeighted {
				edgeWeight = 1
			}

			weight := weights[vertex] + float64(edgeWeight)

			if weight < weights[adjacency] && !hasInfiniteWeight {
				weights[adjacency] = weight
				bestPredecessors[adjacency] = vertex

				// The agenda records the shorter distance for the vertex that
				// has just been relaxed.
				if relax, ok := onHeap[adjacency]; ok {
					relax.priority = weight
					heap.Fix(&items, relax.index)
				}
			}
		}
	}

	path := []K{target}
	current := target

	for current != source {
		// If the current vertex is not present in bestPredecessors, current is
		// set to the zero value of K. Without this check, this would lead to an
		// endless prepending of zero values to the path. Also, the target would
		// not be reachable from one of the preceding vertices.
		if _, ok := bestPredecessors[current]; !ok {
			return nil, ErrTargetNotReachable
		}

		current = bestPredecessors[current]
		path = append([]K{current}, path...)
	}

	return path, nil
}

// StronglyConnectedComponents detects all strongly connected components within
// the graph and returns the hashes of the vertices shaping these components, so
// each component is represented by a []K.
//
// StronglyConnectedComponents can only run on directed graphs.
//
// The whole walk is driven from right here: instead of recursing over the
// adjacency structure, the examination keeps an explicit record of the
// vertices whose successors still have to be looked at.
func StronglyConnectedComponents[K comparable, T any](g Graph[K, T]) ([][]K, error) {
	if !g.Traits().IsDirected {
		return nil, errors.New("SCCs can only be detected in directed graphs")
	}

	adjacencyMap, err := g.AdjacencyMap()
	if err != nil {
		return nil, fmt.Errorf("could not get adjacency map: %w", err)
	}

	components := make([][]K, 0)

	discovered := make(map[K]struct{})
	trailMembers := make(map[K]struct{})
	index := make(map[K]int)
	lowlink := make(map[K]int)
	time := 0

	// The open trail holds the vertices that might still grow into the
	// component being examined; it is consumed from its end.
	trail := make([]K, 0)

	for root := range adjacencyMap {
		if _, ok := discovered[root]; ok {
			continue
		}

		// Instead of a recursive call, the walk keeps its own record of the
		// vertices currently being examined, together with how far each of
		// them has progressed through its successors.
		frames := make([]K, 0, len(adjacencyMap))
		cursors := make([]int, 0, len(adjacencyMap))
		successors := make([][]K, 0, len(adjacencyMap))

		// Enter the root vertex like every other vertex: stamp it, remember
		// it as part of the open trail, and prepare its successor list.
		{
			discovered[root] = struct{}{}
			index[root] = time
			lowlink[root] = time
			time++

			trail = append(trail, root)
			trailMembers[root] = struct{}{}

			frames = append(frames, root)
			cursors = append(cursors, 0)

			rootSuccessors := make([]K, 0, len(adjacencyMap[root]))
			for successor := range adjacencyMap[root] {
				rootSuccessors = append(rootSuccessors, successor)
			}
			successors = append(successors, rootSuccessors)
		}

		for len(frames) > 0 {
			top := len(frames) - 1
			current := frames[top]

			if cursors[top] < len(successors[top]) {
				adjacency := successors[top][cursors[top]]
				cursors[top]++

				if _, ok := discovered[adjacency]; !ok {
					// Descend into the successor: stamp it, remember it on the
					// open trail, and record it as the next frame.
					discovered[adjacency] = struct{}{}
					index[adjacency] = time
					lowlink[adjacency] = time
					time++

					trail = append(trail, adjacency)
					trailMembers[adjacency] = struct{}{}

					frames = append(frames, adjacency)
					cursors = append(cursors, 0)

					adjacencySuccessors := make([]K, 0, len(adjacencyMap[adjacency]))
					for successor := range adjacencyMap[adjacency] {
						adjacencySuccessors = append(adjacencySuccessors, successor)
					}
					successors = append(successors, adjacencySuccessors)
				} else if _, ok := trailMembers[adjacency]; ok {
					// The successor is already on the open trail, so the edge
					// joining the current vertex and the successor is a back
					// edge. Therefore, the lowlink value of the current vertex
					// has to record the index of the successor.
					if index[adjacency] < lowlink[current] {
						lowlink[current] = index[adjacency]
					}
				}

				continue
			}

			// Every successor of the current vertex has been examined, so the
			// walk retreats to the previous vertex.
			frames = frames[:top]
			cursors = cursors[:top]
			successors = successors[:top]

			if top > 0 {
				if predecessor := frames[top-1]; lowlink[current] < lowlink[predecessor] {
					lowlink[predecessor] = lowlink[current]
				}
			}

			// If the lowlink value of the vertex is equal to its index, this
			// is the head vertex of a strongly connected component that's
			// shaped by the vertex and all vertices on the open trail.
			if lowlink[current] == index[current] {
				var component []K
				var popped K

				for popped != current {
					popped = trail[len(trail)-1]
					trail = trail[:len(trail)-1]
					delete(trailMembers, popped)

					component = append(component, popped)
				}

				components = append(components, component)
			}
		}
	}

	return components, nil
}

// AllPathsBetween computes and returns all paths between two given vertices. A
// path is represented as a slice of vertex hashes. The returned slice contains
// these paths.
//
// AllPathsBetween utilizes a non-recursive implementation. It has an
// estimated runtime complexity of O(n^2) where n is the number of vertices.
func AllPathsBetween[K comparable, T any](g Graph[K, T], start, end K) ([][]K, error) {
	adjacencyMap, err := g.AdjacencyMap()
	if err != nil {
		return nil, err
	}

	// The exploration keeps the path being extended in an open slice together
	// with a bookkeeping set of the vertices on it, and records every
	// unfinished branch point in a layer of postponed successors.
	path := make([]K, 0)
	onPath := make(map[K]struct{})
	layers := make([][]K, 0)

	// The start vertex forms the first layer of the path; all of its not yet
	// visited successors are postponed on the first layer.
	path = append(path, start)
	onPath[start] = struct{}{}

	postponed := make([]K, 0)
	for successor := range adjacencyMap[start] {
		if _, ok := onPath[successor]; ok {
			continue
		}
		postponed = append(postponed, successor)
	}
	layers = append(layers, postponed)

	allPaths := make([][]K, 0)

	for len(path) > 0 {
		current := path[len(path)-1]
		open := layers[len(layers)-1]

		if len(open) == 0 {
			if current == end {
				// The current path reaches the end vertex, so it is complete.
				completePath := make([]K, 0, len(path))
				for _, hash := range path {
					completePath = append(completePath, hash)
				}
				allPaths = append(allPaths, completePath)
			}

			// Every postponed successor of the current vertex has been
			// examined, so the top layer is discarded and the exploration
			// retreats by one vertex.
			if len(path) == 0 || len(layers) == 0 {
				return nil, errors.New("unable to remove layer: empty stack")
			}

			path = path[:len(path)-1]
			delete(onPath, current)
			layers = layers[:len(layers)-1]
		} else {
			// Drain the top layer: each postponed successor extends the
			// current path and records its own yet unexplored successors on a
			// new top layer, which the exploration examines next.
			if len(path) == 0 || len(layers) == 0 {
				return nil, errors.New("unable to build stack: empty stack")
			}

			for {
				open = layers[len(layers)-1]
				if len(open) == 0 {
					break
				}

				element := open[len(open)-1]
				layers[len(layers)-1] = open[:len(open)-1]

				path = append(path, element)
				onPath[element] = struct{}{}

				elementSuccessors := make([]K, 0)
				for successor := range adjacencyMap[element] {
					if _, ok := onPath[successor]; ok {
						continue
					}
					elementSuccessors = append(elementSuccessors, successor)
				}
				layers = append(layers, elementSuccessors)
			}
		}
	}

	return allPaths, nil
}
