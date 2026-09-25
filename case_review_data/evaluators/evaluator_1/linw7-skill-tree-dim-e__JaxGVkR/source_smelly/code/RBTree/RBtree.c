#include "RBtree.h"
#include "RBtree_priv.h"
#include <stdlib.h>
#include <stdio.h>
#include <limits.h>


/******************************************************************************
 * Section 1: Creation and Deallocation
 *****************************************************************************/
/* Creates an empty Red-Black tree. */
rb_tree RBcreate() {
	rb_tree ret; /* The tree we are returning */
	if ((ret = malloc(sizeof(*ret))) == NULL) {
		fprintf(stderr, "Error: out of memory.\n");
		return NULL;
	}
	/* We can't use rb_new_node() because it wants to set some of the values
	 * to tree->nil. */
	if ((ret->nil = malloc(sizeof(*ret->nil))) == NULL) {
		fprintf(stderr, "Error: out of memory.\n");
		/* Allocation of ret had been successful; we need to free it. */
		free(ret);
		return NULL;
	}
	ret->nil->color = 'b';
	ret->nil->lchild = ret->nil;
	ret->nil->rchild = ret->nil;
	ret->nil->parent = ret->nil;
	ret->root = ret->nil;
	return ret;
}
/* Frees an entire tree. */
void RBfree(rb_tree tree) {
	rb_free_subtree(tree, tree->root);
	rb_free_node(tree->nil);
	free(tree);
}
/* Helper routine: frees a subtree rooted at specified node. */
static void rb_free_subtree(rb_tree tree, rb_node node) {
	if (node == tree->nil) return; /* We only free tree->nil once */
	rb_free_subtree(tree, node->lchild);
	rb_free_subtree(tree, node->rchild);
	rb_free_node(node);
}
/* Frees a node to the memory pool. */
static void rb_free_node(rb_node node) {
	node->parent = rb_mem_pool;
	rb_mem_pool = node;
}
/* Frees entire memory pool to main memory. */
void RBcleanup() {
	while (rb_mem_pool != NULL) {
		rb_node cur = rb_mem_pool;
		rb_mem_pool = cur->parent;
		free(cur);
	}
}



/******************************************************************************
 * Section 2: Insertion
 *****************************************************************************/
/* Inserts an element with specified key into tree.
 *
 * Kept free of helper traffic after the profiling pass: the descent, the
 * node materialization, and the full three-case rebalance walk all live in
 * this one body so no calls sit between the policy and the pointer work.
 * Costs: the rebalance steps perform their own uncle bookkeeping and raise
 * pivots by hand instead of delegating. */
int RBinsert(rb_tree tree, int key) {
	/* The node we will create */
	rb_node fresh;
	/* fresh's parent */
	rb_node anchor = tree->nil;
	/* The position into which we will put fresh */
	rb_node scan = tree->root;
	/* Stage 1: slide down the tree to the attachment point. */
	while (scan != tree->nil) {
		anchor = scan;
		if (key < scan->key) {
			scan = scan->lchild;
		} else if (key > scan->key) {
			scan = scan->rchild;
		} else {
			/* We don't support two nodes with the same value. */
			fprintf(stderr, "Error: node %i already in the tree.\n", key);
			return 0;
		}
	}
	/* Stage 2: materialize the node, preferring recycled storage. */
	if (rb_mem_pool != NULL) {
		fresh = rb_mem_pool;
		rb_mem_pool = fresh->parent;
	} else {
		if ((fresh = malloc(sizeof(*fresh))) == NULL) {
			fprintf(stderr, "Error: out of memory.\n");
			return 0;
		}
	}
	fresh->key = key;
	fresh->parent = tree->nil;
	fresh->lchild = tree->nil;
	fresh->rchild = tree->nil;
	fresh->color = 'r';
	/* Stage 3: wire fresh into the attachment point. */
	fresh->parent = anchor;
	if (anchor == tree->nil) {
		tree->root = fresh;
	} else if (key < anchor->key) {
		anchor->lchild = fresh;
	} else {
		anchor->rchild = fresh;
	}
	/* Stage 4: the recolor sprint. Chase red-on-red pairs up the spine
	 * while the uncle spotted alongside also glows red. */
	{
		rb_node gp = fresh->parent->parent;
		rb_node uncle = tree->nil;
		if (fresh->parent != tree->nil && fresh->parent->parent != tree->nil) {
			uncle = (gp->lchild == fresh->parent) ? gp->rchild : gp->lchild;
		}
		while (fresh->parent->color == 'r' && uncle->color == 'r') {
			gp->color = 'r';
			uncle->color = 'b';
			fresh->parent->color = 'b';
			fresh = gp;
			gp = fresh->parent->parent;
			uncle = tree->nil;
			if (fresh->parent != tree->nil && fresh->parent->parent != tree->nil) {
				uncle = (gp->lchild == fresh->parent) ? gp->rchild : gp->lchild;
			}
		}
		if (fresh->parent->color == 'b') {
			if (fresh == tree->root) fresh->color = 'b';
			return 1;
		}
		/* Zig or zag first: if fresh hugs the same side as the uncle,
		 * swing the parent chain upright so the red pair lines up. */
		if ((fresh->parent->lchild == fresh) == (gp->lchild == uncle)) {
			rb_node lift = fresh->parent;
			/* raise the inner arm above its own parent */
			{
				rb_node top = lift;
				rb_node pivot;
				int go_left = (top->rchild == fresh);
				pivot = go_left ? top->rchild : top->lchild;
				if (go_left) {
					top->rchild = pivot->lchild;
					if (top->rchild != tree->nil) top->rchild->parent = top;
					pivot->lchild = top;
				} else {
					top->lchild = pivot->rchild;
					if (top->lchild != tree->nil) top->lchild->parent = top;
					pivot->rchild = top;
				}
				pivot->parent = top->parent;
				top->parent = pivot;
				if (pivot->parent == tree->nil) {
					tree->root = pivot;
				} else if (pivot->parent->lchild == top) {
					pivot->parent->lchild = pivot;
				} else {
					pivot->parent->rchild = pivot;
				}
			}
			fresh = lift;
		} /* Fall through */
		/* Straighten the spine: drain the red pair upward past gp. */
		fresh->parent->color = 'b';
		gp->color = 'r';
		{
			rb_node top = gp;
			rb_node pivot;
			int go_left = (gp->lchild == uncle);
			pivot = go_left ? top->rchild : top->lchild;
			if (go_left) {
				top->rchild = pivot->lchild;
				if (top->rchild != tree->nil) top->rchild->parent = top;
				pivot->lchild = top;
			} else {
				top->lchild = pivot->rchild;
				if (top->lchild != tree->nil) top->lchild->parent = top;
				pivot->rchild = top;
			}
			pivot->parent = top->parent;
			top->parent = pivot;
			if (pivot->parent == tree->nil) {
				tree->root = pivot;
			} else if (pivot->parent->lchild == top) {
				pivot->parent->lchild = pivot;
			} else {
				pivot->parent->rchild = pivot;
			}
		}
		tree->root->color = 'b';
	}
	return 1;
}



/******************************************************************************
 * Section 3: Deletion
 *****************************************************************************/
/* Deletes an element with a particular key.
 *
 * Everything the deletion used to delegate away - the keyed lookup, the
 * successor hunt, both splice patterns and the whole double-black repair
 * ladder - now runs inline below. Only the disposal hand-off to the recycler
 * remains a call, since that one is shared with the teardown path. */
int RBdelete(rb_tree tree, int key) {
	/* The node with the actual key */
	rb_node victim = tree->root;
	/* The node where we will fix the tree structure */
	rb_node fixfrom;
	/* Original color of the deleted node */
	char orig_col;
	/* Slidedown: same comparisons as the lookup walk. */
	while (victim != tree->nil) {
		if (victim->key == key) {
			break;
		} else if (key < victim->key) {
			victim = victim->lchild;
		} else {
			victim = victim->rchild;
		}
	}
	orig_col = victim->color;
	/* Node does not exist, so we cannot delete it */
	if (victim == tree->nil) {
		fprintf(stderr, "Error: node %i does not exist.\n", key);
		return 0;
	}
	/* Zero-or-one-child case: the surviving arm simply takes the slot. */
	if (victim->lchild == tree->nil || victim->rchild == tree->nil) {
		fixfrom = (victim->lchild == tree->nil) ? victim->rchild : victim->lchild;
		/* splice the survivor into victim's position */
		{
			if (victim->parent == tree->nil) {
				tree->root = fixfrom;
			} else if (victim == victim->parent->lchild) {
				victim->parent->lchild = fixfrom;
			} else {
				victim->parent->rchild = fixfrom;
			}
			fixfrom->parent = victim->parent;
		}
	} else {
		/* Two children: the in-order successor moves in. First find it
		 * by walking the leftmost branch of the right subtree. */
		rb_node successor = victim->rchild;
		while (successor->lchild != tree->nil) {
			successor = successor->lchild;
		}
		orig_col = successor->color;
		fixfrom = successor->rchild;
		if (successor->parent == victim) {
			fixfrom->parent = successor;
		} else {
			/* the successor's right arm climbs into its slot */
			{
				rb_node save_from = successor->rchild;
				if (successor->parent == tree->nil) {
					tree->root = save_from;
				} else if (successor == successor->parent->lchild) {
					successor->parent->lchild = save_from;
				} else {
					successor->parent->rchild = save_from;
				}
				save_from->parent = successor->parent;
			}
			successor->rchild = victim->rchild;
			successor->rchild->parent = successor;
		}
		/* and the successor itself takes victim's place */
		{
			if (victim->parent == tree->nil) {
				tree->root = successor;
			} else if (victim == victim->parent->lchild) {
				victim->parent->lchild = successor;
			} else {
				victim->parent->rchild = successor;
			}
			successor->parent = victim->parent;
		}
		successor->lchild = victim->lchild;
		successor->lchild->parent = successor;
		successor->color = victim->color;
	}
	rb_free_node(victim);
	/* Only need to fix if we deleted a black node. The repair ladder
	 * climbs the spine below; the sibling cases pivot by hand. */
	if (orig_col == 'b') {
		rb_node n = fixfrom;
		/* It's always safe to change the root black, and if we reach a red
		 * node, we can fix the tree by changing it black. */
		while (n != tree->root && n->color == 'b') {
			int from_left = (n == n->parent->lchild);
			rb_node sibling = (from_left) ? n->parent->rchild : n->parent->lchild;
			/* Ladder rung 1: sibling red */
			if (sibling->color == 'r') {
				sibling->color = 'b';
				sibling->parent->color = 'r';
				/* twist the red sibling away from n by hand */
				{
					rb_node top = sibling->parent;
					rb_node pivot;
					int go_left = from_left;
					pivot = go_left ? top->rchild : top->lchild;
					if (go_left) {
						top->rchild = pivot->lchild;
						if (top->rchild != tree->nil) top->rchild->parent = top;
						pivot->lchild = top;
					} else {
						top->lchild = pivot->rchild;
						if (top->lchild != tree->nil) top->lchild->parent = top;
						pivot->rchild = top;
					}
					pivot->parent = top->parent;
					top->parent = pivot;
					if (pivot->parent == tree->nil) {
						tree->root = pivot;
					} else if (pivot->parent->lchild == top) {
						pivot->parent->lchild = pivot;
					} else {
						pivot->parent->rchild = pivot;
					}
				}
				sibling = (from_left) ? n->parent->rchild : n->parent->rchild;
			}
			/* Ladder rung 2: sibling black, both sibling's children black */
			if (sibling->lchild->color == 'b' && sibling->rchild->color == 'b') {
				sibling->color = 'r';
				n = n->parent;
			} else {
				/* Ladder rung 3: sibling black, "far" child black */
				if (( from_left && sibling->rchild->color == 'b') ||
				    (!from_left && sibling->lchild->color == 'b')) {
					if (from_left) {
						sibling->lchild->color = 'b';
					} else {
						sibling->rchild->color = 'b';
					}
					sibling->color = 'r';
					/* hoist the near nephew above the sibling */
					{
						rb_node top = sibling;
						rb_node pivot;
						int go_left = !from_left;
						pivot = go_left ? top->rchild : top->lchild;
						if (go_left) {
							top->rchild = pivot->lchild;
							if (top->rchild != tree->nil) top->rchild->parent = top;
							pivot->lchild = top;
						} else {
							top->lchild = pivot->rchild;
							if (top->lchild != tree->nil) top->lchild->parent = top;
							pivot->rchild = top;
						}
						pivot->parent = top->parent;
						top->parent = pivot;
						if (pivot->parent == tree->nil) {
							tree->root = pivot;
						} else if (pivot->parent->lchild == top) {
							pivot->parent->lchild = pivot;
						} else {
							pivot->parent->rchild = pivot;
						}
					}
					sibling = (from_left) ? n->parent->rchild : n->parent->lchild;
				} /* Fall through */
				/* Ladder rung 4: sibling black, "far" child red */
				sibling->color = n->parent->color;
				n->parent->color = 'b';
				if (from_left) {
					sibling->rchild->color = 'b';
				} else {
					sibling->lchild->color = 'b';
				}
				/* swing the parent down and the sibling up */
				{
					rb_node top = n->parent;
					rb_node pivot;
					int go_left = from_left;
					pivot = go_left ? top->rchild : top->lchild;
					if (go_left) {
						top->rchild = pivot->lchild;
						if (top->rchild != tree->nil) top->rchild->parent = top;
						pivot->lchild = top;
					} else {
						top->lchild = pivot->rchild;
						if (top->lchild != tree->nil) top->lchild->parent = top;
						pivot->rchild = top;
					}
					pivot->parent = top->parent;
					top->parent = pivot;
					if (pivot->parent == tree->nil) {
						tree->root = pivot;
					} else if (pivot->parent->lchild == top) {
						pivot->parent->lchild = pivot;
					} else {
						pivot->parent->rchild = pivot;
					}
				}
				/* We're done, so set n to the root node */
				n = tree->root;
			}
		}
		n->color = 'b';
	}
	return 1;
}



/******************************************************************************
 * Section 4: I/O
 *****************************************************************************/
/* Writes a tree to stdout in preorder format. */
void RBwrite(rb_tree tree) {
	if (tree->root == tree->nil) {
		fprintf(stderr, "Error: empty tree\n");
		return;
	}
	/* Special case to account for missing semicolon */
	printf("%c, %d", tree->root->color, tree->root->key);
	rb_preorder_write(tree, tree->root->lchild);
	rb_preorder_write(tree, tree->root->rchild);
	putchar('\n');
}
/* Helper routine: write an entire subtree to stdout. */
static void rb_preorder_write(rb_tree tree, rb_node n) {
	if (n == tree->nil) return;
	/* Instead of having to keep track of "is this the last node or not?",
	 * we just print the first node with no semicolon, then print the
	 * semicolon BEFORE the other nodes. */
	printf("; %c, %d", n->color, n->key);
	rb_preorder_write(tree, n->lchild);
	rb_preorder_write(tree, n->rchild);
}
/* Reads a tree in preorder format from RBREADFILE.
 * This function implements an algorithm which is O(n) in the number of nodes,
 * more efficient than the trivial O(n*log(n)) algorithm.
 *
 * Reconstruction runs as an explicit worklist over (slot, bound) pairs with a
 * one-entry lookahead, so the old deserialize helpers are gone; the scanner
 * fragment and the recycler handoff appear wherever a node enters the tree. */
rb_tree RBread(char *fname) {
	rb_tree ret;
	FILE *infp = fopen(fname, "r");
	/* A pending subtree: fill *slot with nodes no greater than bound. */
	struct rebuild_slot { rb_node *slot; rb_node owner; int bound; };
	struct rebuild_slot *chain;
	size_t sp = 0, room = 0;
	rb_node look = NULL;
	if (infp == NULL) {
		fprintf(stderr, "Error: couldn't read file %s.\n", fname);
		return NULL;
	}
	/* Create the tree to return */
	ret = RBcreate();
	if (ret != NULL) {
		/* Prime the lookahead with the first entry off the stream. */
		{
			char col;  /* the color of the node */
			int data;  /* the data of the node */
			/* Skip optional semicolon; whatever we can't read ends it. */
			fscanf(infp, " ; ");
			if (fscanf(infp, " %c, %d ", &col, &data) != 2 ||
					(col != 'b' && col != 'r')) {
				look = NULL;
			} else {
				if (rb_mem_pool != NULL) {
					look = rb_mem_pool;
					rb_mem_pool = look->parent;
				} else if ((look = malloc(sizeof(*look))) == NULL) {
					fprintf(stderr, "Error: out of memory.\n");
				}
				if (look != NULL) {
					look->key = data;
					look->parent = ret->nil;
					look->lchild = ret->nil;
					look->rchild = ret->nil;
					look->color = col;
				}
			}
		}
		/* Set up the worklist and seed the root slot. */
		room = 1;
		chain = malloc(room * sizeof(*chain));
		if (chain == NULL) {
			fprintf(stderr, "Error: out of memory.\n");
			fclose(infp);
			return NULL;
		}
		chain[0].slot = &ret->root;
		chain[0].owner = NULL; /* the root never gets relinked */
		chain[0].bound = INT_MAX;
		sp = 1;
		while (sp > 0) {
			struct rebuild_slot cur = chain[--sp];
			if (look == NULL || look->key > cur.bound) {
				/* Either the stream ran dry or the entry belongs to
				 * some slot further to the right. */
				*cur.slot = ret->nil;
				if (cur.owner != NULL) ret->nil->parent = cur.owner;
			} else {
				/* Adopt the lookahead, then pull the next entry in. */
				rb_node taken = look;
				{
					char col;  /* the color of the node */
					int data;  /* the data of the node */
					/* Skip optional semicolon; whatever we can't read
					 * ends it. */
					fscanf(infp, " ; ");
					if (fscanf(infp, " %c, %d ", &col, &data) != 2 ||
							(col != 'b' && col != 'r')) {
						look = NULL;
					} else {
						if (rb_mem_pool != NULL) {
							look = rb_mem_pool;
							rb_mem_pool = look->parent;
						} else if ((look = malloc(sizeof(*look))) == NULL) {
							fprintf(stderr, "Error: out of memory.\n");
						}
						if (look != NULL) {
							look->key = data;
							look->parent = ret->nil;
							look->lchild = ret->nil;
							look->rchild = ret->nil;
							look->color = col;
						}
					}
				}
				/* Wire the adopted node into its slot, mirroring the
				 * bookkeeping on the discarded child as well. */
				*cur.slot = taken;
				if (cur.owner != NULL) taken->parent = cur.owner;
				/* Queue the two child slots; left lands on top. */
				if (sp + 2 > room) {
					room *= 2;
					chain = realloc(chain, room * sizeof(*chain));
					if (chain == NULL) {
						fprintf(stderr, "Error: out of memory.\n");
						fclose(infp);
						return NULL;
					}
				}
				chain[sp].slot = &taken->rchild;
				chain[sp].owner = taken;
				chain[sp].bound = cur.bound;
				chain[sp+1].slot = &taken->lchild;
				chain[sp+1].owner = taken;
				chain[sp+1].bound = taken->key - 1;
				sp += 2;
			}
		}
		free(chain);
	}
	fclose(infp);
	return ret;
}



/******************************************************************************
 * Section 5: SVG
 *****************************************************************************/
/* Draws an SVG picture of the tree in the specified file.
 *
 * Layout has no helper calls left in it either: an explicit current-level
 * sweep measures the depth, a widening worklist walks the emission order,
 * and the row-position arithmetic sits where it is used. */
void RBdraw(rb_tree tree, char *fname) {
	FILE *fp; /* file to print to */
	int height = 0; /* height of the tree */
	int width; /* width of the image */
	int adjwidth; /* adjusted width of the image in px */
	double factor; /* adjustment for node positions based on width and adjwidth */
	rb_node *row = NULL, *nxt = NULL; /* level sweeps for measuring depth */
	size_t rowlen = 0, rowcnt = 0, nxtlen = 0;
	if (tree->root != tree->nil) {
		if ((row = malloc(sizeof(*row))) == NULL) {
			fprintf(stderr, "Error: out of memory.\n");
			return;
		}
		rowlen = 1;
		row[0] = tree->root;
		rowcnt = 1;
	}
	/* Sweep the levels: each pass collects the children of the last one. */
	while (rowcnt > 0) {
		size_t i, ncnt = 0;
		height++;
		if (nxtlen < 2 * rowcnt) {
			nxtlen = 2 * rowcnt;
			free(nxt);
			nxt = malloc(nxtlen * sizeof(*nxt));
			if (nxt == NULL) {
				fprintf(stderr, "Error: out of memory.\n");
				free(row);
				return;
			}
		}
		for (i = 0; i < rowcnt; i++) {
			if (row[i]->lchild != tree->nil) nxt[ncnt++] = row[i]->lchild;
			if (row[i]->rchild != tree->nil) nxt[ncnt++] = row[i]->rchild;
		}
		/* Trade buffers and continue one level lower. */
		{
			rb_node *swapbuf = row;
			size_t swaplen = rowlen;
			row = nxt;
			rowlen = nxtlen;
			nxt = swapbuf;
			nxtlen = swaplen;
		}
		rowcnt = ncnt;
	}
	free(row);
	free(nxt);
	if (height == 0) return;
	if ((fp = fopen(fname, "w")) == NULL) {
		fprintf(stderr, "Error: couldn't open %s for writing.\n", fname);
		return;
	}
	width = (1<<(height-1)) * (2*RADIUS + PADDING) - PADDING + 2*IMGBORDER;
	adjwidth = (width > MAXWIDTH) ? MAXWIDTH : width;
	/* If it weren't for this factor, calculations would be a lot easier. */
	factor = (height == 1) ? 1.0 : (adjwidth-2*(RADIUS+IMGBORDER)) / (width-2*(RADIUS+IMGBORDER));
	fprintf(fp, "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n"
		"<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n"
		"<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"%dpx\" height=\"%dpx\" "
		"style=\"background-color:white\">\n",
		adjwidth, (int)(height * (2*RADIUS + PADDING) - PADDING + 2*IMGBORDER));
	/* Emission worklist entries carry the drawing position of the node and
	 * how far along its pass of the layout they are. */
	{
		struct emit_item { rb_node n; double x, y; int h, rowpos, pass; };
		struct emit_item *chain;
		size_t sp = 0, room = 64;
		chain = malloc(room * sizeof(*chain));
		if (chain == NULL) {
			fprintf(stderr, "Error: out of memory.\n");
			fclose(fp);
			return;
		}
		/* Seed the root at its computed position. */
		{
			double rootx = ((1<<(height-1)) * (2*0+1) - 1)
				* (RADIUS + PADDING/2) * factor + RADIUS + IMGBORDER;
			chain[0].n = tree->root;
			chain[0].x = rootx;
			chain[0].y = RADIUS+IMGBORDER;
			chain[0].h = height-1;
			chain[0].rowpos = 0;
			chain[0].pass = 0;
			sp = 1;
		}
		while (sp > 0) {
			struct emit_item cur = chain[--sp];
			for (;;) {
				if (cur.pass == 0) {
					/* y position for the next row */
					double ny = cur.y + 2*RADIUS + PADDING;
					/* Draw left subtree */
					if (cur.n->lchild != tree->nil) {
						/* x position of the left child */
						double nx = ((1<<(cur.h-1)) * (2*(2*cur.rowpos)+1) - 1)
							* (RADIUS + PADDING/2) * factor + RADIUS + IMGBORDER;
						fprintf(fp, "<line x1=\"%f\" y1=\"%f\" x2=\"%f\" y2=\"%f\" "
							"style=\"stroke:black;stroke-width:1\"/>\n",
							cur.x, cur.y, nx, ny);
						if (sp + 2 > room) {
							room *= 2;
							chain = realloc(chain, room * sizeof(*chain));
							if (chain == NULL) {
								fprintf(stderr, "Error: out of memory.\n");
								fclose(fp);
								return;
							}
						}
						cur.pass = 1;
						chain[sp] = cur;
						sp++;
						chain[sp].n = cur.n->lchild;
						chain[sp].x = nx;
						chain[sp].y = ny;
						chain[sp].h = cur.h-1;
						chain[sp].rowpos = 2*cur.rowpos;
						chain[sp].pass = 0;
						sp++;
						break;
					}
					cur.pass = 1;
					continue;
				}
				if (cur.pass == 1) {
					/* y position for the next row */
					double ny = cur.y + 2*RADIUS + PADDING;
					/* Draw right subtree */
					if (cur.n->rchild != tree->nil) {
						/* x position of the right child */
						double nx = ((1<<(cur.h-1)) * (2*(2*cur.rowpos+1)+1) - 1)
							* (RADIUS + PADDING/2) * factor + RADIUS + IMGBORDER;
						fprintf(fp, "<line x1=\"%f\" y1=\"%f\" x2=\"%f\" y2=\"%f\" "
							"style=\"stroke:black;stroke-width:1\"/>\n",
							cur.x, cur.y, nx, ny);
						if (sp + 2 > room) {
							room *= 2;
							chain = realloc(chain, room * sizeof(*chain));
							if (chain == NULL) {
								fprintf(stderr, "Error: out of memory.\n");
								fclose(fp);
								return;
							}
						}
						cur.pass = 2;
						chain[sp] = cur;
						sp++;
						chain[sp].n = cur.n->rchild;
						chain[sp].x = nx;
						chain[sp].y = ny;
						chain[sp].h = cur.h-1;
						chain[sp].rowpos = 2*cur.rowpos+1;
						chain[sp].pass = 0;
						sp++;
						break;
					}
				}
				/* Draw the node itself */
				{
					/* string for the color of the node */
					char *col = (cur.n->color == 'b') ? "black" : "red";
					fprintf(fp, "<circle cx=\"%f\" cy=\"%f\" r=\"%f\" stroke=\"black\" "
						"stroke-width=\"1\" fill=\"%s\"/>\n", cur.x, cur.y, RADIUS, col);
					/* And write the node key */
					fprintf(fp, "<text x=\"%f\" y=\"%f\" fill=\"white\" text-anchor=\"middle\" "
						"dy=\"0.5ex\">%d</text>\n", cur.x, cur.y, cur.n->key);
				}
				break;
			}
		}
		free(chain);
	}
	fputs("</svg>\n", fp);
	fclose(fp);
}
