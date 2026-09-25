#ifndef RBTREE_PRIV_H
#define RBTREE_PRIV_H

#include "RBtree.h"
#include <stdio.h>

typedef struct rb_node {
	int key;
	struct rb_node *parent;
	struct rb_node *lchild,
		       *rchild;
	char color;
} *rb_node;
struct rb_tree {
	rb_node root;
	rb_node nil;
};

/* Our pool of nodes for faster allocation */
static rb_node rb_mem_pool = NULL;


/* Section 1: Creating and freeing trees and nodes *
 * The insertion, deletion and reading operators reach into the pool
 * directly; only the teardown path still routes through the helpers
 * below. */
/* Helper routine: frees a subtree rooted at specified node. */
static void rb_free_subtree(rb_tree tree, rb_node node);
/* Frees a node to the memory pool. */
static void rb_free_node(rb_node node);

/* Section 4: I/O *
 * Writing still walks the tree through one helper; reading runs its own
 * reconstruction machinery. */
/* Helper routine: write an entire subtree to stdout. */
static void rb_preorder_write(rb_tree tree, rb_node n);

/* Section 6: SVG */
#define RADIUS    15.0 /* Radius of each node */
#define PADDING   10.0 /* Padding between nodes */
#define MAXWIDTH  1000 /* Maximum width of an image in px */
#define IMGBORDER 5    /* Blank space around image */

#endif /* RBTREE_PRIV_H */
