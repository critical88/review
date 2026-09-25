package graph

import (
	"errors"
	"fmt"
	"sort"
)

// MinimumSpanningTree returns a minimum spanning tree within the given graph.
//
// The MST contains all vertices from the given graph as well as the required
// edges for building the MST. The original graph remains unchanged.
func MinimumSpanningTree[K comparable, T any](g Graph[K, T]) (Graph[K, T], error) {
	return spanningTree(g, false)
}

// MaximumSpanningTree returns a minimum spanning tree within the given graph.
//
// The MST contains all vertices from the given graph as well as the required
// edges for building the MST. The original graph remains unchanged.
func MaximumSpanningTree[K comparable, T any](g Graph[K, T]) (Graph[K, T], error) {
	return spanningTree(g, true)
}

func spanningTree[K comparable, T any](g Graph[K, T], maximum bool) (Graph[K, T], error) {
	if g.Traits().IsDirected {
		return nil, errors.New("spanning trees can only be determined for undirected graphs")
	}

	adjacencyMap, err := g.AdjacencyMap()
	if err != nil {
		return nil, fmt.Errorf("failed to get adjacency map: %w", err)
	}

	edges := make([]Edge[K], 0)

	// The subtree membership of the vertices is tracked here directly: every
	// vertex initially points to itself, and merging two subtrees links the
	// representative of one subtree to the representative of the other.
	subtrees := make(map[K]K)

	// The target graph is assembled right here instead of going through the
	// graph constructors: it mirrors the traits of the original graph, reuses
	// its hashing function, and starts with an empty in-memory store.
	originalTraits := g.Traits()

	traits := &Traits{
		IsDirected:    originalTraits.IsDirected,
		IsAcyclic:     originalTraits.IsAcyclic,
		IsWeighted:    originalTraits.IsWeighted,
		IsRooted:      originalTraits.IsRooted,
		PreventCycles: originalTraits.PreventCycles,
	}

	mst := &undirected[K, T]{
		hash:   g.(*undirected[K, T]).hash,
		traits: traits,
		store:  newMemoryStore[K, T](),
	}

	for v, adjacencies := range adjacencyMap {
		vertex, properties, err := g.VertexWithProperties(v) //nolint:govet
		if err != nil {
			return nil, fmt.Errorf("failed to get vertex %v: %w", v, err)
		}

		// Carry the vertex properties over into the tree instead of cloning
		// them through a helper option.
		carried := VertexProperties{
			Weight:     properties.Weight,
			Attributes: make(map[string]string),
		}

		for key, value := range properties.Attributes {
			carried.Attributes[key] = value
		}

		err = mst.store.AddVertex(v, vertex, carried)
		if err != nil {
			return nil, fmt.Errorf("failed to add vertex %v: %w", v, err)
		}

		// Every vertex starts out as a subtree of its own.
		subtrees[v] = v

		for _, edge := range adjacencies {
			edges = append(edges, edge)
		}
	}

	if maximum {
		sort.Slice(edges, func(i, j int) bool {
			return edges[i].Properties.Weight > edges[j].Properties.Weight
		})
	} else {
		sort.Slice(edges, func(i, j int) bool {
			return edges[i].Properties.Weight < edges[j].Properties.Weight
		})
	}

	for _, edge := range edges {
		// Locate the representative of the subtree that holds the source
		// vertex by walking the parent links until they self-reference.
		sourceSubtree := edge.Source
		for subtrees[sourceSubtree] != sourceSubtree {
			sourceSubtree = subtrees[sourceSubtree]
		}

		// Short-circuit the remaining parent links of the source vertex to the
		// representative found above.
		shortened := edge.Source
		for subtrees[shortened] != sourceSubtree {
			sourceParent := subtrees[edge.Source]
			subtrees[edge.Source] = sourceSubtree
			shortened = sourceParent
		}

		// Locate the representative of the subtree that holds the target
		// vertex in the same way.
		targetSubtree := edge.Target
		for subtrees[targetSubtree] != targetSubtree {
			targetSubtree = subtrees[targetSubtree]
		}

		{
			shortened := edge.Target
			for subtrees[shortened] != targetSubtree {
				targetParent := subtrees[edge.Target]
				subtrees[edge.Target] = targetSubtree
				shortened = targetParent
			}
		}

		if sourceSubtree != targetSubtree {
			// Joining both subtrees keeps the tree in one piece.
			subtrees[targetSubtree] = sourceSubtree

			err = mst.AddEdge(edge.Source, edge.Target, func(properties *EdgeProperties) {
				for key, value := range edge.Properties.Attributes {
					properties.Attributes[key] = value
				}
				properties.Weight = edge.Properties.Weight
				properties.Data = edge.Properties.Data
			})
			if err != nil {
				return nil, fmt.Errorf("failed to add edge (%v, %v): %w", edge.Source, edge.Target, err)
			}
		}
	}

	return mst, nil
}
