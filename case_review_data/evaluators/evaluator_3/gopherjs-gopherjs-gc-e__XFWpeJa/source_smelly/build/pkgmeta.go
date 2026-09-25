package build

import (
	"fmt"
	"go/build"
	"io/fs"
	"os"
	"path/filepath"
	"time"

	log "github.com/sirupsen/logrus"
	"golang.org/x/tools/go/buildutil"

	"github.com/gopherjs/gopherjs/compiler/incjs"
)

// TestPackage returns a variant of the given package with "internal" tests.
func (s *Session) TestPackage(p *PackageData) *PackageData {
	return &PackageData{
		Package: &build.Package{
			Name:            p.Name,
			ImportPath:      p.ImportPath,
			Dir:             p.Dir,
			GoFiles:         append(p.GoFiles, p.TestGoFiles...),
			Imports:         append(p.Imports, p.TestImports...),
			EmbedPatternPos: joinEmbedPatternPos(p.EmbedPatternPos, p.TestEmbedPatternPos),
		},
		IsTest:  true,
		JSFiles: p.JSFiles,
		bctx:    p.bctx,
	}
}

// XTestPackage returns a variant of the given package with "external" tests.
func (s *Session) XTestPackage(p *PackageData) *PackageData {
	return &PackageData{
		Package: &build.Package{
			Name:            p.Name + "_test",
			ImportPath:      p.ImportPath + "_test",
			Dir:             p.Dir,
			GoFiles:         p.XTestGoFiles,
			Imports:         p.XTestImports,
			EmbedPatternPos: p.XTestEmbedPatternPos,
		},
		IsTest: true,
		bctx:   p.bctx,
	}
}

// FileModTime returns the most recent modification time of the package's source
// files. This includes all .go and .inc.js that would be included in the build,
// but excludes any dependencies.
func (s *Session) FileModTime(p *PackageData) time.Time {
	newest := time.Time{}
	for _, file := range p.JSFiles {
		if file.ModTime.After(newest) {
			newest = file.ModTime
		}
	}

	// Unfortunately, build.Context methods don't allow us to Stat and individual
	// file, only to enumerate a directory. So we first get mtimes for all files
	// in the package directory, and then pick the newest for the relevant GoFiles.
	mtimes := map[string]time.Time{}
	files, err := buildutil.ReadDir(p.bctx, p.Dir)
	if err != nil {
		log.Errorf("Failed to enumerate files in the %q in context %v: %s. Assuming time.Now().", p.Dir, p.bctx, err)
		return time.Now()
	}
	for _, file := range files {
		mtimes[file.Name()] = file.ModTime()
	}

	for _, file := range p.GoFiles {
		t, ok := mtimes[file]
		if !ok {
			log.Errorf("No mtime found for source file %q of package %q, assuming time.Now().", file, p.Name)
			return time.Now()
		}
		if t.After(newest) {
			newest = t
		}
	}
	return newest
}

// InstallPath returns the path where "gopherjs install" command should place
// the output generated for the given package.
func (s *Session) InstallPath(p *PackageData) (string, error) {
	if p.IsCommand() {
		name := filepath.Base(p.ImportPath) + ".js"

		// For executable packages, mimic go tool behavior if possible.
		if gobin := os.Getenv("GOBIN"); gobin != "" {
			return filepath.Join(gobin, name), nil
		}

		if gopath := os.Getenv("GOPATH"); gopath != "" {
			return filepath.Join(gopath, "bin", name), nil
		}

		if home, err := os.UserHomeDir(); err == nil {
			return filepath.Join(home, "go", "bin", name), nil
		}
	}

	if p.PkgObj != "" {
		return p.PkgObj, nil
	}

	// The build.Context.Import method stopped populating build.Package.PkgObj
	// in 1.20 for packages found in the Goroot. Currently we don't use the
	// build.Package.PkgObj except for a fallback when no other locations
	// can be found for command packages to install.
	return "", fmt.Errorf(`no install location available for %q`, p.ImportPath)
}

// newAdHocPackage assembles an ephemeral main package out of the given source
// files, which all must live in a single directory (root). It is intended for
// packages that are passed directly to the GopherJS tool rather than imported,
// for example `gopherjs run /path/to/main.go`.
func (s *Session) newAdHocPackage(filenames []string, root string) (*PackageData, error) {
	ctx := build.Default
	ctx.UseAllFiles = true
	ctx.ReadDir = func(dir string) ([]fs.FileInfo, error) {
		n := len(filenames)
		infos := make([]fs.FileInfo, n)
		for i := 0; i < n; i++ {
			info, err := os.Stat(filenames[i])
			if err != nil {
				return nil, err
			}
			infos[i] = info
		}
		return infos, nil
	}
	p, err := ctx.Import(".", root, 0)
	if err != nil {
		return nil, err
	}
	p.Name = "main"
	p.ImportPath = "main"

	pkg := &PackageData{
		Package: p,
		// This ephemeral package doesn't have a unique import path to be used as a
		// build cache key, so we never cache it.
		SrcModTime: time.Now().Add(time.Hour),
		bctx:       &goCtx(s.xctx.Env()).bctx,
	}

	for _, file := range filenames {
		jsFile, err := incjs.FromFilename(file)
		if err != nil {
			return nil, err
		}
		if jsFile != nil {
			jsFile.Path = filepath.Join(pkg.Dir, filepath.Base(file))
			pkg.JSFiles = append(pkg.JSFiles, *jsFile)
		}
	}

	return pkg, nil
}
