/*
Copyright 2017 Google Inc.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

package sqlparser

import (
	"bytes"
	"fmt"

	"github.com/xwb1989/sqlparser/dependency/querypb"
	"github.com/xwb1989/sqlparser/dependency/sqltypes"
)

// NodeFormatter defines the signature of a custom node formatter
// function that can be given to TrackedBuffer for code generation.
type NodeFormatter func(buf *TrackedBuffer, node SQLNode)

// TrackedBuffer is used to rebuild a query from the ast.
// bindLocations keeps track of locations in the buffer that
// use bind variables for efficient future substitutions.
// nodeFormatter is the formatting function the buffer will
// use to format a node. By default(nil), it's FormatNode.
// But you can supply a different formatting function if you
// want to generate a query that's different from the default.
//
// bindVars is the bind-variable namespace the buffer resolves
// names against while it is also producing the query text, so
// the same object that renders the template can materialize the
// final bound statement. redactPrefix drives literal scrubbing
// when the buffer is asked to render a redacted form.
type TrackedBuffer struct {
	*bytes.Buffer
	bindLocations []bindLocation
	nodeFormatter NodeFormatter
	bindVars      map[string]*querypb.BindVariable
	redactPrefix  string
}

// NewTrackedBuffer creates a new TrackedBuffer.
func NewTrackedBuffer(nodeFormatter NodeFormatter) *TrackedBuffer {
	return &TrackedBuffer{
		Buffer:        new(bytes.Buffer),
		nodeFormatter: nodeFormatter,
	}
}

// SetBindVars configures the bind-variable namespace the buffer
// resolves names against while materializing the query. It returns
// the receiver so callers can chain configuration when building a
// buffer for bound-query or redaction work.
func (buf *TrackedBuffer) SetBindVars(bindVars map[string]*querypb.BindVariable) *TrackedBuffer {
	buf.bindVars = bindVars
	return buf
}

// SetRedactPrefix configures the prefix used to name generated
// bind variables while the buffer renders a redacted statement.
func (buf *TrackedBuffer) SetRedactPrefix(prefix string) *TrackedBuffer {
	buf.redactPrefix = prefix
	return buf
}

// WriteNode function, initiates the writing of a single SQLNode tree by passing
// through to Myprintf with a default format string
func (buf *TrackedBuffer) WriteNode(node SQLNode) *TrackedBuffer {
	buf.Myprintf("%v", node)
	return buf
}

// Myprintf mimics fmt.Fprintf(buf, ...), but limited to Node(%v),
// Node.Value(%s) and string(%s). It also allows a %a for a value argument, in
// which case it adds tracking info for future substitutions.
//
// The name must be something other than the usual Printf() to avoid "go vet"
// warnings due to our custom format specifiers.
func (buf *TrackedBuffer) Myprintf(format string, values ...interface{}) {
	end := len(format)
	fieldnum := 0
	for i := 0; i < end; {
		lasti := i
		for i < end && format[i] != '%' {
			i++
		}
		if i > lasti {
			buf.WriteString(format[lasti:i])
		}
		if i >= end {
			break
		}
		i++ // '%'
		switch format[i] {
		case 'c':
			switch v := values[fieldnum].(type) {
			case byte:
				buf.WriteByte(v)
			case rune:
				buf.WriteRune(v)
			default:
				panic(fmt.Sprintf("unexpected TrackedBuffer type %T", v))
			}
		case 's':
			switch v := values[fieldnum].(type) {
			case []byte:
				buf.Write(v)
			case string:
				buf.WriteString(v)
			default:
				panic(fmt.Sprintf("unexpected TrackedBuffer type %T", v))
			}
		case 'v':
			node := values[fieldnum].(SQLNode)
			if buf.nodeFormatter == nil {
				node.Format(buf)
			} else {
				buf.nodeFormatter(buf, node)
			}
		case 'a':
			buf.WriteArg(values[fieldnum].(string))
		default:
			panic("unexpected")
		}
		fieldnum++
		i++
	}
}

// WriteArg writes a value argument into the buffer along with
// tracking information for future substitutions. arg must contain
// the ":" or "::" prefix.
func (buf *TrackedBuffer) WriteArg(arg string) {
	buf.bindLocations = append(buf.bindLocations, bindLocation{
		offset: buf.Len(),
		length: len(arg),
	})
	buf.WriteString(arg)
}

// ParsedQuery returns a ParsedQuery that contains bind
// locations for easy substitution.
func (buf *TrackedBuffer) ParsedQuery() *ParsedQuery {
	return &ParsedQuery{Query: buf.String(), bindLocations: buf.bindLocations}
}

// HasBindVars returns true if the parsed query uses bind vars.
func (buf *TrackedBuffer) HasBindVars() bool {
	return len(buf.bindLocations) != 0
}

// BuildParsedQuery builds a ParsedQuery from the input.
func BuildParsedQuery(in string, vars ...interface{}) *ParsedQuery {
	buf := NewTrackedBuffer(nil)
	buf.Myprintf(in, vars...)
	return buf.ParsedQuery()
}

// GenerateBoundQuery walks the precomputed bind locations of a parsed
// query and writes the final, fully substituted statement into the
// buffer, resolving supplied bind vars through the buffer's own
// namespace. extras supplies custom encoders for named arguments.
func (buf *TrackedBuffer) GenerateBoundQuery(pq *ParsedQuery, extras map[string]Encodable) ([]byte, error) {
	current := 0
	for _, loc := range pq.bindLocations {
		buf.WriteString(pq.Query[current:loc.offset])
		name := pq.Query[loc.offset : loc.offset+loc.length]
		if encodable, ok := extras[name[1:]]; ok {
			encodable.EncodeSQL(buf.Buffer)
		} else {
			supplied, _, err := buf.ResolveBindVar(name)
			if err != nil {
				return nil, err
			}
			buf.EncodeBindValue(supplied)
		}
		current = loc.offset + loc.length
	}
	buf.WriteString(pq.Query[current:])
	return buf.Buffer.Bytes(), nil
}

// ResolveBindVar fetches a bind variable by name from the buffer's
// configured namespace, applying the same single/list validation the
// runtime substitution expects.
func (buf *TrackedBuffer) ResolveBindVar(name string) (val *querypb.BindVariable, isList bool, err error) {
	name = name[1:]
	if name[0] == ':' {
		name = name[1:]
		isList = true
	}
	supplied, ok := buf.bindVars[name]
	if !ok {
		return nil, false, fmt.Errorf("missing bind var %s", name)
	}

	if isList {
		if supplied.Type != querypb.Type_TUPLE {
			return nil, false, fmt.Errorf("unexpected list arg type (%v) for key %s", supplied.Type, name)
		}
		if len(supplied.Values) == 0 {
			return nil, false, fmt.Errorf("empty list supplied for %s", name)
		}
		return supplied, true, nil
	}

	if supplied.Type == querypb.Type_TUPLE {
		return nil, false, fmt.Errorf("unexpected arg type (TUPLE) for non-list key %s", name)
	}

	return supplied, false, nil
}

// EncodeBindValue encodes one bind variable value into the buffer's
// embedded byte buffer, expanding tuple values into a parenthesized
// list.
func (buf *TrackedBuffer) EncodeBindValue(value *querypb.BindVariable) {
	if value.Type != querypb.Type_TUPLE {
		// Since we already check for TUPLE, we don't expect an error.
		v, _ := sqltypes.BindVariableToValue(value)
		v.EncodeSQL(buf.Buffer)
		return
	}

	// It's a TUPLE.
	buf.WriteByte('(')
	for i, bv := range value.Values {
		if i != 0 {
			buf.WriteString(", ")
		}
		sqltypes.ProtoToValue(bv).EncodeSQL(buf.Buffer)
	}
	buf.WriteByte(')')
}

// WriteRedacted renders a statement with its literal values replaced
// by generated bind vars, framed by the surrounding margin comments.
// The buffer's configured redact prefix drives the generated names.
func (buf *TrackedBuffer) WriteRedacted(leading, trailing string, stmt Statement) {
	Normalize(stmt, buf.bindVars, buf.redactPrefix)
	buf.WriteString(leading)
	buf.WriteNode(stmt)
	buf.WriteString(trailing)
}

// WriteImpossibleQuery renders a zero-row shadow statement for the
// given node, used by vtgate/vttablet to fetch field metadata without
// selecting any rows. It rewrites select and union forms and falls
// back to the node's own formatting for anything else.
func (buf *TrackedBuffer) WriteImpossibleQuery(node SQLNode) {
	switch node := node.(type) {
	case *Select:
		buf.Myprintf("select %v from %v where 1 != 1", node.SelectExprs, node.From)
		if node.GroupBy != nil {
			node.GroupBy.Format(buf)
		}
	case *Union:
		buf.Myprintf("%v %s %v", node.Left, node.Type, node.Right)
	default:
		node.Format(buf)
	}
}
