// Package build implements GopherJS build system.
//
// WARNING: This package's API is treated as internal and currently doesn't
// provide any API stability guarantee, use it at your own risk. If you need a
// stable interface, prefer invoking the gopherjs CLI tool as a subprocess.
package build

import (
	"fmt"
	"go/ast"
	"go/build"
	"go/token"
	"go/types"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/fsnotify/fsnotify"

	"github.com/gopherjs/gopherjs/build/cache"
	"github.com/gopherjs/gopherjs/compiler"
	"github.com/gopherjs/gopherjs/compiler/incjs"
	"github.com/gopherjs/gopherjs/compiler/sources"
	"github.com/gopherjs/gopherjs/internal/sourcemapx"
	"github.com/gopherjs/gopherjs/internal/testmain"
)

// DefaultGOROOT is the default GOROOT value for builds.
//
// It uses the GOPHERJS_GOROOT environment variable if it is set,
// or else the default GOROOT value of the system Go distribution.
var DefaultGOROOT = func() string {
	if goroot, ok := os.LookupEnv("GOPHERJS_GOROOT"); ok {
		// GopherJS-specific GOROOT value takes precedence.
		return goroot
	}
	// The usual default GOROOT.
	return build.Default.GOROOT
}()

// NewBuildContext creates a build context for building Go packages
// with GopherJS compiler.
//
// Core GopherJS packages (i.e., "github.com/gopherjs/gopherjs/js", "github.com/gopherjs/gopherjs/nosync")
// are loaded from gopherjspkg.FS virtual filesystem if not present in GOPATH or
// go.mod.
func NewBuildContext(installSuffix string, buildTags []string) XContext {
	e := DefaultEnv()
	e.InstallSuffix = installSuffix
	e.BuildTags = buildTags
	realGOROOT := goCtx(e)
	return &chainedCtx{
		primary:   realGOROOT,
		secondary: gopherjsCtx(e),
	}
}

// Import returns details about the Go package named by the import path. If the
// path is a local import path naming a package that can be imported using
// a standard import path, the returned package will set p.ImportPath to
// that path.
//
// In the directory containing the package, .go and .inc.js files are
// considered part of the package except for:
//
//   - .go files in package documentation
//   - files starting with _ or . (likely editor temporary files)
//   - files with build constraints not satisfied by the context
//
// If an error occurs, Import returns a non-nil error and a nil
// *PackageData.
func Import(path string, mode build.ImportMode, installSuffix string, buildTags []string) (*PackageData, error) {
	wd, err := os.Getwd()
	if err != nil {
		// Getwd may fail if we're in GOOS=js mode. That's okay, handle
		// it by falling back to empty working directory. It just means
		// Import will not be able to resolve relative import paths.
		wd = ""
	}
	xctx := NewBuildContext(installSuffix, buildTags)
	return xctx.Import(path, wd, mode)
}

// exclude returns files, excluding specified files.
func exclude(files []string, exclude ...string) []string {
	var s []string
Outer:
	for _, f := range files {
		for _, e := range exclude {
			if f == e {
				continue Outer
			}
		}
		s = append(s, f)
	}
	return s
}

// ImportDir is like Import but processes the Go package found in the named
// directory.
func ImportDir(dir string, mode build.ImportMode, installSuffix string, buildTags []string) (*PackageData, error) {
	xctx := NewBuildContext(installSuffix, buildTags)
	pkg, err := xctx.Import(".", dir, mode)
	if err != nil {
		return nil, err
	}

	return pkg, nil
}

// Options controls build process behavior.
type Options struct {
	Verbose        bool
	Quiet          bool
	Watch          bool
	CreateMapFile  bool
	MapToLocalDisk bool
	Minify         bool
	Color          bool
	BuildTags      []string
	TestedPackage  string
	NoCache        bool
}

// PrintError message to the terminal.
func (o *Options) PrintError(format string, a ...any) {
	if o.Color {
		format = "\x1B[31m" + format + "\x1B[39m"
	}
	fmt.Fprintf(os.Stderr, format, a...)
}

// PrintSuccess message to the terminal.
func (o *Options) PrintSuccess(format string, a ...any) {
	if o.Color {
		format = "\x1B[32m" + format + "\x1B[39m"
	}
	fmt.Fprintf(os.Stderr, format, a...)
}

// PackageData is an extension of go/build.Package with additional metadata
// GopherJS requires.
type PackageData struct {
	*build.Package
	JSFiles []incjs.File
	// IsTest is true if the package is being built for running tests.
	IsTest     bool
	SrcModTime time.Time
	UpToDate   bool
	// If true, the package does not have a corresponding physical directory on disk.
	IsVirtual bool

	bctx *build.Context // The original build context this package came from.
}

func (p PackageData) String() string {
	return fmt.Sprintf("%s [is_test=%v]", p.ImportPath, p.IsTest)
}

// InternalBuildContext returns the build context that produced the package.
//
// WARNING: This function is a part of internal API and will be removed in
// future.
func (p *PackageData) InternalBuildContext() *build.Context {
	return p.bctx
}

// Session manages internal state GopherJS requires to perform a build.
//
// This is the main interface to GopherJS build system. Session lifetime is
// roughly equivalent to a single GopherJS tool invocation.
//
// Beyond driving the overall build, the session owns the package assembly
// pipeline end to end: it parses and augments package sources with the
// GopherJS native overlays, synthesizes initializers for go:embed directives,
// produces the internal and external test variants of a package, keeps track
// of source freshness and decides where command packages are installed. All
// per-build state is kept on the session so every part of the pipeline can
// reach it through a single object.
type Session struct {
	options    *Options
	xctx       XContext
	buildCache cache.Cache

	// importPaths is a map of the resolved import paths given the
	// source directory (first key) and the unresolved import path (second key).
	// This is used to cache the resolved import returned from XContext.Import.
	// XContent.Import can be slow, so we cache the resolved path that is used
	// as the map key by parsedPackages and UpToDateArchives.
	// This makes subsequent lookups faster during compilation when all we have
	// is the unresolved import path and source directory.
	importPaths map[string]map[string]string

	// augOverrides and augFound carry the native overlay augmentation state for
	// the package that the session is currently assembling. They are reset at
	// the start of every parseAndAugment pass and are only meaningful while the
	// session is inside the augmentation pipeline.
	augOverrides map[string]overrideInfo
	augFound     map[string]struct{}

	// packages caches imported package metadata by resolved import path.
	// This avoids repeated xctx.Import calls for the same package.
	packages map[string]*PackageData

	// sources is a map of parsed packages that have been built and augmented.
	// This is keyed using resolved import paths. This is used to avoid
	// rebuilding and augmenting packages that are imported by several packages.
	// The files in these sources haven't been sorted nor simplified yet.
	sources map[string]*sources.Sources

	// Binary archives produced during the current session and assumed to be
	// up to date with input sources and dependencies. In the -w ("watch") mode
	// must be cleared upon entering watching.
	UpToDateArchives map[string]*compiler.Archive
	Watcher          *fsnotify.Watcher
}

// NewSession creates a new GopherJS build session.
func NewSession(options *Options) (*Session, error) {
	options.Verbose = options.Verbose || options.Watch

	s := &Session{
		options:          options,
		importPaths:      make(map[string]map[string]string),
		packages:         make(map[string]*PackageData),
		sources:          make(map[string]*sources.Sources),
		UpToDateArchives: make(map[string]*compiler.Archive),
	}
	s.xctx = NewBuildContext(s.InstallSuffix(), s.options.BuildTags)
	env := s.xctx.Env()

	// Go distribution version check.
	if err := compiler.CheckGoVersion(env.GOROOT); err != nil {
		return nil, err
	}

	// If the cache is enabled, initialize the build cache.
	// Disable caching by leaving buildCache set to nil.
	//
	// TODO(grantnelson-wf): Currently the build cache is slower than
	// parsing and augmenting the files, so we disable it for now.
	// Re-enable it once the cache performance is improved.
	const disableDefaultCache = true
	if !s.options.NoCache && !disableDefaultCache {
		s.buildCache = &cache.BuildCache{
			GOOS:          env.GOOS,
			GOARCH:        env.GOARCH,
			GOROOT:        env.GOROOT,
			GOPATH:        env.GOPATH,
			BuildTags:     append([]string{}, env.BuildTags...),
			TestedPackage: options.TestedPackage,
			Version:       compiler.Version,
		}
	}

	if options.Watch {
		if out, err := exec.Command("ulimit", "-n").Output(); err == nil {
			if n, err := strconv.Atoi(strings.TrimSpace(string(out))); err == nil && n < 1024 {
				fmt.Printf("Warning: The maximum number of open file descriptors is very low (%d). Change it with 'ulimit -n 8192'.\n", n)
			}
		}

		var err error
		s.Watcher, err = fsnotify.NewWatcher()
		if err != nil {
			return nil, err
		}
	}
	return s, nil
}

// XContext returns the session's build context.
func (s *Session) XContext() XContext { return s.xctx }

// InstallSuffix returns the suffix added to the generated output file.
func (s *Session) InstallSuffix() string {
	if s.options.Minify {
		return "min"
	}
	return ""
}

// GoRelease returns Go release version this session is building with.
func (s *Session) GoRelease() string {
	return compiler.GoRelease(s.xctx.Env().GOROOT)
}

// TestBinary returns the testBinary value that is normally passed into
// cmd/link by a -X option. The testBinary value is defined in the STL at
// testing/testing.go for the `testing.Testing() bool` function.
// It is set to "1" if the binary (the JS output in our case) is built
// with "go test" and "0" otherwise.
func (s *Session) TestBinary() string {
	if s.options.TestedPackage != `` {
		return `1`
	}
	return `0`
}

// BuildFiles passed to the GopherJS tool as if they were a package.
//
// A ephemeral package will be created with only the provided files. This
// function is intended for use with, for example, `gopherjs run main.go`.
func (s *Session) BuildFiles(filenames []string, pkgObj string, cwd string) error {
	if len(filenames) == 0 {
		return fmt.Errorf("no input sources are provided")
	}

	normalizedDir := func(filename string) string {
		d := filepath.Dir(filename)
		if !filepath.IsAbs(d) {
			d = filepath.Join(cwd, d)
		}
		return filepath.Clean(d)
	}

	// Ensure all source files are in the same directory.
	dirSet := map[string]bool{}
	for _, file := range filenames {
		dirSet[normalizedDir(file)] = true
	}
	dirList := []string{}
	for dir := range dirSet {
		dirList = append(dirList, dir)
	}
	sort.Strings(dirList)
	if len(dirList) != 1 {
		return fmt.Errorf("named files must all be in one directory; have: %v", strings.Join(dirList, ", "))
	}

	root := dirList[0]
	pkg, err := s.newAdHocPackage(filenames, root)
	if err != nil {
		return err
	}

	archive, err := s.BuildProject(pkg)
	if err != nil {
		return err
	}
	if s.sources["main"].Package.Name() != "main" {
		return fmt.Errorf("cannot build/run non-main package")
	}
	return s.WriteCommandPackage(archive, pkgObj)
}

// BuildProject builds a command project (one with a main method) or
// builds a test project (one with a synthesized test main package).
func (s *Session) BuildProject(pkg *PackageData) (*compiler.Archive, error) {
	// ensure that runtime for gopherjs is imported
	pkg.Imports = append(pkg.Imports, `runtime`)

	// Load the project to get the sources for the parsed packages.
	var rootSrcs *sources.Sources
	var err error
	if pkg.IsTest {
		rootSrcs, err = s.loadTestPackage(pkg)
	} else {
		rootSrcs, err = s.LoadPackages(pkg)
	}
	if err != nil {
		return nil, err
	}

	// Compile the project into Archives containing the generated JS.
	return s.prepareAndCompilePackages(rootSrcs)
}

// GetSortedSources returns the sources sorted by import path.
// The files in the sources may still not be sorted yet.
func (s *Session) GetSortedSources() []*sources.Sources {
	allSources := make([]*sources.Sources, 0, len(s.sources))
	for _, srcs := range s.sources {
		allSources = append(allSources, srcs)
	}
	sources.SortedSourcesSlice(allSources)
	return allSources
}

func (s *Session) loadTestPackage(pkg *PackageData) (*sources.Sources, error) {
	_, err := s.LoadPackages(s.TestPackage(pkg))
	if err != nil {
		return nil, err
	}
	_, err = s.LoadPackages(s.XTestPackage(pkg))
	if err != nil {
		return nil, err
	}

	// Generate a synthetic testmain package.
	fset := token.NewFileSet()
	tests := testmain.TestMain{Package: pkg.Package, Context: pkg.bctx}
	tests.Scan(fset)
	mainPkg, mainFile, err := tests.Synthesize(fset)
	if err != nil {
		return nil, fmt.Errorf("failed to generate testmain package for %s: %w", pkg.ImportPath, err)
	}

	// Create the sources for parsed package for the testmain package.
	srcs := &sources.Sources{
		ImportPath: mainPkg.ImportPath,
		Dir:        mainPkg.Dir,
		Files:      []*ast.File{mainFile},
		FileSet:    fset,
	}
	s.sources[srcs.ImportPath] = srcs

	// Import dependencies for the testmain package.
	for _, importedPkgPath := range srcs.UnresolvedImports() {
		_, _, err := s.loadImportPathWithSrcDir(importedPkgPath, pkg.Dir)
		if err != nil {
			return nil, err
		}
	}

	return srcs, nil
}

// loadImportPathWithSrcDir gets the parsed package specified by the import path.
//
// Relative import paths are interpreted relative to the passed srcDir.
// If srcDir is empty, current working directory is assumed.
func (s *Session) loadImportPathWithSrcDir(path, srcDir string) (*PackageData, *sources.Sources, error) {
	if pkg, ok := s.cachedPackageFor(path, srcDir); ok {
		// Cache this srcDir lookup as well, so later getImportPath lookups avoid xctx.Import.
		s.cacheImportPath(path, srcDir, pkg.ImportPath)
		srcs, err := s.LoadPackages(pkg)
		if err != nil {
			return nil, nil, err
		}
		return pkg, srcs, nil
	}

	pkg, err := s.xctx.Import(path, srcDir, 0)
	if s.Watcher != nil && pkg != nil { // add watch even on error
		s.Watcher.Add(pkg.Dir)
	}
	if err != nil {
		return nil, nil, err
	}

	s.packages[pkg.ImportPath] = pkg
	srcs, err := s.LoadPackages(pkg)
	if err != nil {
		return nil, nil, err
	}

	s.cacheImportPath(path, srcDir, pkg.ImportPath)
	return pkg, srcs, nil
}

func (s *Session) cachedPackageFor(path, srcDir string) (*PackageData, bool) {
	if resolved, ok := s.importPaths[srcDir][path]; ok {
		pkg, found := s.packages[resolved]
		return pkg, found
	}

	if !build.IsLocalImport(path) {
		pkg, found := s.packages[path]
		return pkg, found
	}

	return nil, false
}

// cacheImportPath stores the resolved import path for the build package
// so we can look it up later without getting the whole build package.
// The given path and source directly are the ones passed into
// XContext.Import to the get the build package originally.
func (s *Session) cacheImportPath(path, srcDir, importPath string) {
	if paths, ok := s.importPaths[srcDir]; ok {
		paths[path] = importPath
	} else {
		s.importPaths[srcDir] = map[string]string{path: importPath}
	}
}

// getExeModTime will determine the mod time of the GopherJS binary
// the first time this is called and cache the result for subsequent calls.
var getExeModTime = func() func() time.Time {
	var (
		once   sync.Once
		result time.Time
	)
	getTime := func() {
		gopherjsBinary, err := os.Executable()
		if err == nil {
			var fileInfo os.FileInfo
			fileInfo, err = os.Stat(gopherjsBinary)
			if err == nil {
				result = fileInfo.ModTime()
				return
			}
		}
		os.Stderr.WriteString("Could not get GopherJS binary's modification timestamp. Please report issue.\n")
		result = time.Now()
	}
	return func() time.Time {
		once.Do(getTime)
		return result
	}
}()

// LoadPackages will recursively load and parse the given package and
// its dependencies. This will return the sources for the given package.
// The returned source and sources for the dependencies will be added
// to the session's sources map.
func (s *Session) LoadPackages(pkg *PackageData) (*sources.Sources, error) {
	if srcs, ok := s.sources[pkg.ImportPath]; ok {
		return srcs, nil
	}

	if exeModTime := getExeModTime(); exeModTime.After(pkg.SrcModTime) {
		pkg.SrcModTime = exeModTime
	}

	for _, importedPkgPath := range pkg.Imports {
		if importedPkgPath == "unsafe" {
			continue
		}
		importedPkg, _, err := s.loadImportPathWithSrcDir(importedPkgPath, pkg.Dir)
		if err != nil {
			return nil, err
		}

		if impModTime := importedPkg.SrcModTime; impModTime.After(pkg.SrcModTime) {
			pkg.SrcModTime = impModTime
		}
	}

	if fileModTime := s.FileModTime(pkg); fileModTime.After(pkg.SrcModTime) {
		pkg.SrcModTime = fileModTime
	}

	// Try to load the package from the build cache.
	var srcs *sources.Sources
	if s.buildCache != nil {
		cachedSrcs := &sources.Sources{}
		if s.buildCache.Load(cachedSrcs, pkg.ImportPath, pkg.SrcModTime) {
			srcs = cachedSrcs
		}
	}

	// If the package was not found in the cache, build the package
	// by parsing and augmenting the original files with overlay files.
	if srcs == nil {
		fileSet := token.NewFileSet()
		files, overlayJsFiles, err := s.parseAndAugment(pkg, pkg.IsTest, fileSet)
		if err != nil {
			return nil, err
		}
		embed, err := s.embedFiles(pkg, fileSet, files)
		if err != nil {
			return nil, err
		}
		if embed != nil {
			files = append(files, embed)
		}

		srcs = &sources.Sources{
			ImportPath: pkg.ImportPath,
			Dir:        pkg.Dir,
			Files:      files,
			FileSet:    fileSet,
			JSFiles:    append(pkg.JSFiles, overlayJsFiles...),
		}

		// Store the built package in the cache for future use.
		if s.buildCache != nil {
			s.buildCache.Store(srcs, srcs.ImportPath, time.Now())
		}
	}

	// Add the sources to the session's sources map.
	s.sources[pkg.ImportPath] = srcs

	// Import dependencies from the augmented files,
	// whilst skipping any that have been already imported.
	for _, importedPkgPath := range srcs.UnresolvedImports(pkg.Imports...) {
		_, _, err := s.loadImportPathWithSrcDir(importedPkgPath, pkg.Dir)
		if err != nil {
			return nil, err
		}
	}

	return srcs, nil
}

func (s *Session) prepareAndCompilePackages(rootSrcs *sources.Sources) (*compiler.Archive, error) {
	tContext := types.NewContext()
	allSources := s.GetSortedSources()

	// Prepare and analyze the source code.
	// This will be performed recursively for all dependencies.
	if err := compiler.PrepareAllSources(allSources, s.SourcesForImport, tContext); err != nil {
		return nil, err
	}

	// Compile all the sources into archives.
	for _, srcs := range allSources {
		if _, err := s.compilePackage(srcs, tContext); err != nil {
			return nil, err
		}
	}

	rootArchive, ok := s.UpToDateArchives[rootSrcs.ImportPath]
	if !ok {
		// This is confirmation that the root package is in the sources map and got compiled.
		return nil, fmt.Errorf(`root package %q was not found in archives`, rootSrcs.ImportPath)
	}
	return rootArchive, nil
}

func (s *Session) compilePackage(srcs *sources.Sources, tContext *types.Context) (*compiler.Archive, error) {
	if archive, ok := s.UpToDateArchives[srcs.ImportPath]; ok {
		return archive, nil
	}

	archive, err := compiler.Compile(srcs, tContext, s.options.Minify)
	if err != nil {
		return nil, err
	}

	if s.options.Verbose {
		fmt.Println(srcs.ImportPath)
	}

	s.UpToDateArchives[srcs.ImportPath] = archive

	return archive, nil
}

func (s *Session) getImportPath(path, srcDir string) (string, error) {
	// If path is for an xtest package, just return it.
	if strings.HasSuffix(path, "_test") {
		return path, nil
	}

	// Check if the import path is already cached.
	if importPath, ok := s.importPaths[srcDir][path]; ok {
		return importPath, nil
	}

	// Fast-path for non-local imports when we already have their metadata.
	if !build.IsLocalImport(path) {
		if _, ok := s.sources[path]; ok {
			// Sources are keyed by canonical import path.
			s.cacheImportPath(path, srcDir, path)
			return path, nil
		}
		if pkg, ok := s.packages[path]; ok {
			s.cacheImportPath(path, srcDir, pkg.ImportPath)
			return pkg.ImportPath, nil
		}
	}

	// Fall back to the slow import of the build package.
	pkg, err := s.xctx.Import(path, srcDir, 0)
	if err != nil {
		return ``, err
	}
	s.cacheImportPath(path, srcDir, pkg.ImportPath)
	return pkg.ImportPath, nil
}

func (s *Session) SourcesForImport(path, srcDir string) (*sources.Sources, error) {
	importPath, err := s.getImportPath(path, srcDir)
	if err != nil {
		return nil, err
	}

	srcs, ok := s.sources[importPath]
	if !ok {
		return nil, fmt.Errorf(`sources for %q not found`, path)
	}

	return srcs, nil
}

// ImportResolverFor returns a function which returns a compiled package archive
// given an import path.
func (s *Session) ImportResolverFor(srcDir string) func(string) (*compiler.Archive, error) {
	return func(path string) (*compiler.Archive, error) {
		importPath, err := s.getImportPath(path, srcDir)
		if err != nil {
			return nil, err
		}

		if archive, ok := s.UpToDateArchives[importPath]; ok {
			return archive, nil
		}

		return nil, fmt.Errorf(`archive for %q not found`, importPath)
	}
}

// SourceMappingCallback returns a callback for [github.com/gopherjs/gopherjs/compiler.SourceMapFilter]
// configured for the current build session.
func (s *Session) EnableMapping(filter *sourcemapx.Filter, jsFileName string) {
	filter.EnableMapping(jsFileName, s.xctx.Env().GOROOT, s.xctx.Env().GOPATH, s.options.MapToLocalDisk)
}

// WriteCommandPackage writes the final JavaScript output file at pkgObj path.
func (s *Session) WriteCommandPackage(archive *compiler.Archive, pkgObj string) error {
	if err := os.MkdirAll(filepath.Dir(pkgObj), 0o777); err != nil {
		return err
	}
	codeFile, err := os.Create(pkgObj)
	if err != nil {
		return err
	}
	defer codeFile.Close()

	sourceMapFilter := &sourcemapx.Filter{Writer: codeFile}
	if s.options.CreateMapFile {
		s.EnableMapping(sourceMapFilter, filepath.Base(pkgObj))

		mapFile, err := os.Create(pkgObj + ".map")
		if err != nil {
			return err
		}

		defer func() {
			sourceMapFilter.WriteMappingTo(mapFile)
			mapFile.Close()
			fmt.Fprintf(codeFile, "//# sourceMappingURL=%s.map\n", filepath.Base(pkgObj))
		}()
	}

	deps, err := compiler.ImportDependencies(archive, s.ImportResolverFor(""))
	if err != nil {
		return err
	}
	return compiler.WriteProgramCode(deps, sourceMapFilter, s.GoRelease(), s.TestBinary())
}

// WaitForChange watches file system events and returns if either when one of
// the source files is modified.
func (s *Session) WaitForChange() {
	// Will need to re-validate up-to-dateness of all archives, so flush them from
	// memory.
	s.importPaths = map[string]map[string]string{}
	s.sources = map[string]*sources.Sources{}
	s.UpToDateArchives = map[string]*compiler.Archive{}

	s.options.PrintSuccess("watching for changes...\n")
	for {
		select {
		case ev := <-s.Watcher.Events:
			if ev.Op&(fsnotify.Create|fsnotify.Write|fsnotify.Remove|fsnotify.Rename) == 0 || filepath.Base(ev.Name)[0] == '.' {
				continue
			}
			if !strings.HasSuffix(ev.Name, ".go") && !strings.HasSuffix(ev.Name, incjs.Ext) {
				continue
			}
			s.options.PrintSuccess("change detected: %s\n", ev.Name)
		case err := <-s.Watcher.Errors:
			s.options.PrintError("watcher error: %s\n", err.Error())
		}
		break
	}

	go func() {
		for range s.Watcher.Events {
			// consume, else Close() may deadlock
		}
	}()
	s.Watcher.Close()
}
