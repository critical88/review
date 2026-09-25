package main

import (
	"fmt"
	"io"
	"log"
	"os"

	"github.com/microcosm-cc/bluemonday"
)

func main() {
	// Define a policy, we are using the UGC policy as a base.
	p := bluemonday.UGCPolicy()

	// Add "rel=nofollow" to links
	p.RequireNoFollowOnLinks(true)
	p.RequireNoFollowOnFullyQualifiedLinks(true)

	// Open external links in a new window/tab
	p.AddTargetBlankToFullyQualifiedLinks(true)

	// From here the policy is handled only as a definer, so that the runner
	// below is not tied to the concrete policy type.
	var pd bluemonday.PolicyDefiner = p

	// Read input from stdin so that this is a nice unix utility and can receive
	// piped input, then sanitise it and write it to stdout
	if err := sanitizeInput(pd, os.Stdin, os.Stdout); err != nil {
		log.Fatal(err)
	}
}

// sanitizeInput sanitises everything that can be read from in and writes the
// sanitised document to out.
func sanitizeInput(pd bluemonday.PolicyDefiner, in io.Reader, out io.Writer) error {
	dirty, err := io.ReadAll(in)
	if err != nil {
		return err
	}

	// Apply the policy and write to stdout
	_, err = fmt.Fprint(
		out,
		pd.Sanitize(
			string(dirty),
		),
	)

	return err
}
