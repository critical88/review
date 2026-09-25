package tree

import (
	"strings"

	"charm.land/lipgloss/v2"
)

// StyleFunc allows the tree to be styled per item.
type StyleFunc func(children Children, i int) lipgloss.Style

// Style is the styling applied to the tree.
type Style struct {
	enumeratorFunc StyleFunc
	indenterFunc   StyleFunc
	itemFunc       StyleFunc
	root           lipgloss.Style
}

// enumPrefix renders the enumerator prefix of the item at index i.
func enumPrefix(children Children, i int, enumerator Enumerator, enumeratorFunc StyleFunc) string {
	prefix := enumerator(children, i)
	return enumeratorFunc(children, i).Render(prefix)
}

// maxEnumPrefix returns the children with hidden trailing items removed, along
// with the width of the widest rendered enumerator prefix.
func maxEnumPrefix(children Children, enumerator Enumerator, enumeratorFunc StyleFunc) (Children, int) {
	var maxLen int
	for i := range children.Length() {
		if i < children.Length()-1 {
			if child := children.At(i + 1); child.Hidden() {
				// Don't count the last child if its hidden. This renders the
				// last visible element with the right prefix
				//
				// The only type of Children is NodeChildren.
				children = children.(NodeChildren).Remove(i + 1)
			}
		}
		prefix := enumPrefix(children, i, enumerator, enumeratorFunc)
		maxLen = max(lipgloss.Width(prefix), maxLen)
	}
	return children, maxLen
}

// renderTree is responsible for actually rendering a tree node with the given
// rendering configuration.
func renderTree(node Node, root bool, prefix string, enumerator Enumerator, indenter Indenter, rootStyle lipgloss.Style, enumeratorFunc, indenterFunc, itemFunc StyleFunc, width int) string {
	if node.Hidden() {
		return ""
	}

	children := node.Children()

	strs := make([]string, 0, children.Length())

	// print the root node name if its not empty.
	if name := node.Value(); name != "" && root {
		line := rootStyle.Render(name)
		// If the line is shorter than the desired width, we pad it with spaces.
		if pad := width - lipgloss.Width(line); pad > 0 {
			line = name + rootStyle.Render(strings.Repeat(" ", pad))
		}
		strs = append(strs, rootStyle.Render(line))
	}

	children, maxLen := maxEnumPrefix(children, enumerator, enumeratorFunc)

	strs = append(
		strs,
		renderChildren(
			children,
			prefix,
			enumerator,
			indenter,
			rootStyle,
			enumeratorFunc,
			indenterFunc,
			itemFunc,
			width,
			maxLen,
		)...,
	)

	return strings.Join(strs, "\n")
}

// renderChildren renders the visible children of a node, recursing into each
// child subtree with its own rendering configuration when it has one.
func renderChildren(children Children, prefix string, enumerator Enumerator, indenter Indenter, rootStyle lipgloss.Style, enumeratorFunc, indenterFunc, itemFunc StyleFunc, width int, maxLen int) []string {
	strs := make([]string, 0, children.Length())
	for i := range children.Length() {
		child := children.At(i)
		if child.Hidden() {
			continue
		}
		line, indent := renderChild(children, i, prefix, enumerator, indenter, enumeratorFunc, indenterFunc, itemFunc, width, maxLen)
		strs = append(strs, line)

		if children.Length() > 0 {
			// Here we see if the child has a custom render configuration, which
			// means the user set a custom enumerator/indenter/item style, etc.
			// If it has one, we'll use it to render itself.
			// otherwise, we keep using the current configuration.
			// Note that the configuration doesn't inherit its parent's.
			childEnumerator, childIndenter, childRootStyle := enumerator, indenter, rootStyle
			childEnumeratorFunc, childIndenterFunc, childItemFunc := enumeratorFunc, indenterFunc, itemFunc
			childWidth := width
			switch child := child.(type) {
			case *Tree:
				if child.renderSet {
					childEnumerator = child.enumerator
					childIndenter = child.indenter
					childRootStyle = child.style.root
					childEnumeratorFunc = child.style.enumeratorFunc
					childIndenterFunc = child.style.indenterFunc
					childItemFunc = child.style.itemFunc
					childWidth = child.width
				}
			}
			if s := renderTree(
				child,
				false,
				prefix+indent,
				childEnumerator,
				childIndenter,
				childRootStyle,
				childEnumeratorFunc,
				childIndenterFunc,
				childItemFunc,
				childWidth,
			); s != "" {
				strs = append(strs, s)
			}
		}
	}
	return strs
}

// renderChild renders the child at index i, returning the rendered line along
// with the indent to use when rendering the child's own children.
func renderChild(children Children, i int, prefix string, enumerator Enumerator, indenter Indenter, enumeratorFunc, indenterFunc, itemFunc StyleFunc, width int, maxLen int) (string, string) {
	child := children.At(i)
	indentStyle := indenterFunc(children, i)
	enumStyle := enumeratorFunc(children, i)

	itemStyle := itemFunc(children, i)

	indent := indentStyle.Render(indenter(children, i))
	nodePrefix := enumPrefix(children, i, enumerator, enumeratorFunc)

	// Preserve the background color of the enumerator when adding the padding
	enumBgStyle := lipgloss.NewStyle().Background(enumStyle.GetBackground())

	// Add padding to the left of the node to align it with the longest prefix of its siblings
	if l := maxLen - lipgloss.Width(nodePrefix); l > 0 {
		nodePrefix = enumBgStyle.Render(strings.Repeat(" ", l)) + nodePrefix
	}

	item := itemStyle.Render(child.Value())
	multineLinePrefix := enumBgStyle.Render(prefix)

	// This dance below is to account for multiline prefixes, e.g. "|\n|".
	// In that case, we need to make sure that both the parent prefix and
	// the current node's prefix have the same height.
	for lipgloss.Height(item) > lipgloss.Height(nodePrefix) {
		nodePrefix = lipgloss.JoinVertical(
			lipgloss.Left,
			nodePrefix,
			indent,
		)
	}
	for lipgloss.Height(nodePrefix) > lipgloss.Height(multineLinePrefix) {
		multineLinePrefix = lipgloss.JoinVertical(
			lipgloss.Left,
			multineLinePrefix,
			prefix,
		)
	}

	line := lipgloss.JoinHorizontal(
		lipgloss.Top,
		multineLinePrefix,
		nodePrefix,
		item,
	)

	// If the line is shorter than the desired width, we pad it with spaces.
	if pad := width - lipgloss.Width(line); pad > 0 {
		line = line + itemStyle.Render(strings.Repeat(" ", pad))
	}

	return line, indent
}
