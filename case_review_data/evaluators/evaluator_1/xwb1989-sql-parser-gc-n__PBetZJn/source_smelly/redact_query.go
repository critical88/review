package sqlparser

import querypb "github.com/xwb1989/sqlparser/dependency/querypb"

// RedactSQLQuery returns a sql string with the params stripped out for display
func RedactSQLQuery(sql string) (string, error) {
	sqlStripped, comments := SplitMarginComments(sql)

	stmt, err := Parse(sqlStripped)
	if err != nil {
		return "", err
	}

	buf := NewTrackedBuffer(nil)
	buf.SetBindVars(map[string]*querypb.BindVariable{}).SetRedactPrefix("redacted")
	buf.WriteRedacted(comments.Leading, comments.Trailing, stmt)

	return buf.String(), nil
}
