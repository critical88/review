package build

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/scanner"
	"go/token"
	"path"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/gopherjs/gopherjs/compiler/astutil"
	"github.com/gopherjs/gopherjs/compiler/errlist"
	"github.com/gopherjs/gopherjs/compiler/incjs"
	"golang.org/x/tools/go/buildutil"
)

// overrideInfo is used by the Session's parseAndAugment methods to manage
// directives and how the overlay and original are merged.
type overrideInfo struct {
	// replace indicates that the original code is expected to exist.
	// If the original code is not present, then error to let us know
	// the original code has been removed or renamed.
	// This will be set to false once the original code has been found.
	replace bool

	// new indicates that the override code is expected to be new.
	// If the original code is present, then error to let us know
	// that we are overriding something that we did not expect to replace.
	// This lets us know of new or renamed code in the original that happens
	// to collide with our overrides.
	new bool

	// keepOriginal indicates that the original code should be kept
	// but the identifier will be prefixed by `_gopherjs_original_foo`.
	// If false the original code is removed.
	keepOriginal bool

	// purgeMethods indicates that this info is for a type and if a method has
	// this type as a receiver then the method should also be removed.
	// If the method is defined in the overlays and therefore has its
	// own overrides, this will be ignored.
	purgeMethods bool

	// overrideSignature is the function definition given in the overlays
	// that should be used to replace the signature in the originals.
	// Only receivers, type parameters, parameters, and results will be used.
	overrideSignature *ast.FuncDecl
}

// parseAndAugment parses and returns all .go files of given pkg.
// Standard Go library packages are augmented with files in compiler/natives folder.
// If isTest is true and pkg.ImportPath has no _test suffix, package is built for running internal tests.
// If isTest is true and pkg.ImportPath has _test suffix, package is built for running external tests.
//
// The native packages are augmented by the contents of natives.FS in the following way.
// The file names do not matter except the usual `_test` suffix. The files for
// native overrides get added to the package (even if they have the same name
// as an existing file from the standard library).
//
//   - For function identifiers that exist in the original and the overrides
//     and have the directive `gopherjs:keep-original`, the original identifier
//     in the AST gets prefixed by `_gopherjs_original_`.
//   - For identifiers that exist in the original and the overrides, and have
//     the directive `gopherjs:purge`, both the original and override are
//     removed. This is for completely removing something which is currently
//     invalid for GopherJS. For any purged types any methods with that type as
//     the receiver are also removed.
//   - For function identifiers that exist in the original and the overrides,
//     and have the directive `gopherjs:override-signature`, the overridden
//     function is removed and the original function's signature is changed
//     to match the overridden function signature. This allows the receiver,
//     type parameters, parameter, and return values to be modified as needed.
//   - Otherwise for identifiers that exist in the original and the overrides,
//     the original is removed. Use `gopherjs:replace` to ensure that the
//     original existed so that a replace is made.
//   - New identifiers that don't exist in original package get added.
//     Use `gopherjs:new` to ensure that the identifier is new and there was
//     no original code for it.
func (s *Session) parseAndAugment(pkg *PackageData, isTest bool, fileSet *token.FileSet) ([]*ast.File, []incjs.File, error) {
	// The overrides and the found sets are internal per-package state of the
	// augmentation pass. Reset both so a session can safely be reused for
	// more than one package.
	s.augOverrides = make(map[string]overrideInfo)
	s.augFound = nil

	jsFiles, overlayFiles := s.parseOverlayFiles(pkg, isTest, fileSet)

	originalFiles, err := s.parserOriginalFiles(pkg, fileSet)
	if err != nil {
		return nil, nil, err
	}

	for _, file := range overlayFiles {
		s.augmentOverlayFile(file)
	}
	delete(s.augOverrides, "init")

	for _, file := range originalFiles {
		s.augmentOriginalImports(pkg.ImportPath, file)
	}

	if len(s.augOverrides) > 0 {
		s.augFound = make(map[string]struct{}, len(s.augOverrides))
		for _, file := range originalFiles {
			s.augmentOriginalFile(file)
		}
		err := s.checkOverrides(pkg.ImportPath)
		if err != nil {
			return nil, nil, err
		}
	}

	return append(overlayFiles, originalFiles...), jsFiles, nil
}

// parseOverlayFiles loads and parses overlay files
// to augment the original files with.
func (s *Session) parseOverlayFiles(pkg *PackageData, isTest bool, fileSet *token.FileSet) ([]incjs.File, []*ast.File) {
	isXTest := strings.HasSuffix(pkg.ImportPath, "_test")
	importPath := pkg.ImportPath
	if isXTest {
		importPath = importPath[:len(importPath)-5]
	}

	nativesContext := overlayCtx(s.xctx.Env())
	nativesPkg, err := nativesContext.Import(importPath, "", 0)
	if err != nil {
		return nil, nil
	}

	jsFiles := nativesPkg.JSFiles
	var files []*ast.File
	names := nativesPkg.GoFiles
	if isTest {
		names = append(names, nativesPkg.TestGoFiles...)
	}
	if isXTest {
		names = nativesPkg.XTestGoFiles
	}

	for _, name := range names {
		fullPath := path.Join(nativesPkg.Dir, name)
		r, err := nativesContext.bctx.OpenFile(fullPath)
		if err != nil {
			panic(err)
		}
		// Files should be uniquely named and in the original package directory in order to be
		// ordered correctly
		newPath := path.Join(pkg.Dir, "gopherjs__"+name)
		file, err := parser.ParseFile(fileSet, newPath, r, parser.ParseComments)
		if err != nil {
			panic(err)
		}
		r.Close()

		files = append(files, file)
	}
	return jsFiles, files
}

// parserOriginalFiles loads and parses the original files to augment.
func (s *Session) parserOriginalFiles(pkg *PackageData, fileSet *token.FileSet) ([]*ast.File, error) {
	var files []*ast.File
	var errList errlist.ErrorList
	for _, name := range pkg.GoFiles {
		if !filepath.IsAbs(name) { // name might be absolute if specified directly. E.g., `gopherjs build /abs/file.go`.
			name = filepath.Join(pkg.Dir, name)
		}

		r, err := buildutil.OpenFile(pkg.bctx, name)
		if err != nil {
			return nil, err
		}

		file, err := parser.ParseFile(fileSet, name, r, parser.ParseComments)
		r.Close()
		if err != nil {
			if list, isList := err.(scanner.ErrorList); isList {
				if len(list) > 10 {
					list = append(list[:10], &scanner.Error{Pos: list[9].Pos, Msg: "too many errors"})
				}
				for _, entry := range list {
					errList = append(errList, entry)
				}
				continue
			}
			errList = append(errList, err)
			continue
		}

		files = append(files, file)
	}

	if errList != nil {
		return nil, errList
	}
	return files, nil
}

// augmentOverlayFile is the part of parseAndAugment that processes
// an overlay file AST to collect information such as compiler directives
// and perform any initial augmentation needed to the overlay.
//
// The collected override information is stored on the session in
// s.augOverrides.
func (s *Session) augmentOverlayFile(file *ast.File) {
	anyChange := false
	for i, decl := range file.Decls {
		replaceDecl := astutil.DirectiveReplace(decl)
		newDecl := astutil.DirectiveNew(decl)
		purgeDecl := astutil.Purge(decl)

		switch d := decl.(type) {
		case *ast.FuncDecl:
			k := astutil.FuncKey(d)
			oi := overrideInfo{
				keepOriginal: astutil.KeepOriginal(d),
				replace:      replaceDecl,
				new:          newDecl,
			}
			if astutil.OverrideSignature(d) {
				oi.overrideSignature = d
				purgeDecl = true
			}
			s.augOverrides[k] = oi
		case *ast.GenDecl:
			for j, spec := range d.Specs {
				purgeSpec := purgeDecl || astutil.Purge(spec)
				replaceSpec := replaceDecl || astutil.DirectiveReplace(spec)
				newSpec := newDecl || astutil.DirectiveNew(spec)
				switch t := spec.(type) {
				case *ast.TypeSpec:
					s.augOverrides[t.Name.Name] = overrideInfo{
						purgeMethods: purgeSpec,
						replace:      replaceSpec,
						new:          newSpec,
					}
				case *ast.ValueSpec:
					for _, name := range t.Names {
						s.augOverrides[name.Name] = overrideInfo{
							replace: replaceSpec,
							new:     newSpec,
						}
					}
				}
				if purgeSpec {
					anyChange = true
					d.Specs[j] = nil
				}
			}
		}
		if purgeDecl {
			anyChange = true
			file.Decls[i] = nil
		}
	}
	if anyChange {
		s.finalizeRemovals(file)
		s.pruneImports(file)
	}
}

// augmentOriginalImports is the part of parseAndAugment that processes
// an original file AST to modify the imports for that file.
func (s *Session) augmentOriginalImports(importPath string, file *ast.File) {
	switch importPath {
	case "crypto/rand", "encoding/gob", "encoding/json", "expvar", "go/token", "log", "math/big", "math/rand", "regexp", "time":
		for _, spec := range file.Imports {
			path, _ := strconv.Unquote(spec.Path.Value)
			if path == "sync" {
				if spec.Name == nil {
					spec.Name = ast.NewIdent("sync")
				}
				spec.Path.Value = `"github.com/gopherjs/gopherjs/nosync"`
			}
		}
	}
}

// augmentOriginalFile is the part of parseAndAugment that processes an
// original file AST to augment the source code using the overrides from
// the overlay files.
//
// The overrides and found maps key with an identifier that uniquely identifies
// the top-level object being augmented.
// The s.augOverrides map should be populated with the overrides to apply
// and s.augFound will be populated with the objects that had an override
// applied.
func (s *Session) augmentOriginalFile(file *ast.File) {
	anyChange := false
	for i, decl := range file.Decls {
		switch d := decl.(type) {
		case *ast.FuncDecl:
			funcKey := astutil.FuncKey(d)
			if info, ok := s.augOverrides[funcKey]; ok {
				s.augFound[funcKey] = struct{}{}
				anyChange = true
				removeFunc := true
				if info.keepOriginal {
					// Allow overridden function calls
					// The standard library implementation of foo() becomes _gopherjs_original_foo()
					d.Name.Name = "_gopherjs_original_" + d.Name.Name
					removeFunc = false
				}
				if overSig := info.overrideSignature; overSig != nil {
					d.Recv = overSig.Recv
					d.Type.TypeParams = overSig.Type.TypeParams
					d.Type.Params = overSig.Type.Params
					d.Type.Results = overSig.Type.Results
					removeFunc = false
				}
				if removeFunc {
					file.Decls[i] = nil
				}
			} else if recvKey := astutil.FuncReceiverKey(d); len(recvKey) > 0 {
				// check if the receiver has been purged, if so, remove the method too.
				if info, ok := s.augOverrides[recvKey]; ok && info.purgeMethods {
					s.augFound[recvKey] = struct{}{}
					anyChange = true
					file.Decls[i] = nil
				}
			}
		case *ast.GenDecl:
			for j, spec := range d.Specs {
				switch t := spec.(type) {
				case *ast.TypeSpec:
					if _, ok := s.augOverrides[t.Name.Name]; ok {
						s.augFound[t.Name.Name] = struct{}{}
						anyChange = true
						d.Specs[j] = nil
					}
				case *ast.ValueSpec:
					if len(t.Names) == len(t.Values) {
						// multi-value context
						// e.g. var a, b = 2, foo[int]()
						// A removal will also remove the value which may be from a
						// function call. This allows us to remove unwanted statements.
						// However, if that call has a side effect which still needs
						// to be run, add the call into the overlay.
						for k, name := range t.Names {
							if _, ok := s.augOverrides[name.Name]; ok {
								s.augFound[name.Name] = struct{}{}
								anyChange = true
								t.Names[k] = nil
								t.Values[k] = nil
							}
						}
					} else {
						// single-value context
						// e.g. var a, b = foo[int]()
						// If a removal from the overlays makes all returned values unused,
						// then remove the function call as well. This allows us to stop
						// unwanted calls if needed. If that call has a side effect which
						// still needs to be run, add the call into the overlay.
						nameRemoved := false
						for _, name := range t.Names {
							if _, ok := s.augOverrides[name.Name]; ok {
								s.augFound[name.Name] = struct{}{}
								nameRemoved = true
								name.Name = `_`
							}
						}
						if nameRemoved {
							removeSpec := true
							for _, name := range t.Names {
								if name.Name != `_` {
									removeSpec = false
									break
								}
							}
							if removeSpec {
								anyChange = true
								d.Specs[j] = nil
							}
						}
					}
				}
			}
		}
	}
	if anyChange {
		s.finalizeRemovals(file)
		s.pruneImports(file)
	}
}

// checkOverrides performs a final check of the overrides to ensure that
// all overrides that were expected to be found were found and all overrides
// that were not expected to be found were not found.
//
// The overrides and found maps key with an identifier
// that uniquely identifies the top-level object being augmented.
// s.augFound is populated with the objects that had an override applied
// so the found keys should be a subset of the keys in the s.augOverrides map.
func (s *Session) checkOverrides(pkgPath string) error {
	el := errlist.ErrorList{}
	for name, info := range s.augOverrides {
		_, wasFound := s.augFound[name]
		switch {
		case wasFound && info.new:
			el = el.Append(fmt.Errorf("gopherjs: original code for %s.%s was found, but override had `new` directive", pkgPath, name))
		case !wasFound && info.replace:
			el = el.Append(fmt.Errorf("gopherjs: original code for %s.%s was not found, but override had `replace` directive", pkgPath, name))
		case !wasFound && info.keepOriginal:
			el = el.Append(fmt.Errorf("gopherjs: original code for %s.%s was not found, but override had `keep-original` directive", pkgPath, name))
		case !wasFound && info.purgeMethods:
			el = el.Append(fmt.Errorf("gopherjs: original code for %s.%s was not found, but override had `purge` directive", pkgPath, name))
		case !wasFound && info.overrideSignature != nil:
			el = el.Append(fmt.Errorf("gopherjs: original code for %s.%s was not found, but override had `override-signature` directive", pkgPath, name))
		}
	}
	return el.ErrOrNil()
}

// isOnlyImports determines if this file is empty except for imports.
func (s *Session) isOnlyImports(file *ast.File) bool {
	for _, decl := range file.Decls {
		if gen, ok := decl.(*ast.GenDecl); ok && gen.Tok == token.IMPORT {
			continue
		}

		// The decl was either a FuncDecl or a non-import GenDecl.
		return false
	}
	return true
}

// pruneImports will remove any unused imports from the file.
//
// This will not remove any dot (`.`) or blank (`_`) imports, unless
// there are no declarations or directives meaning that all the imports
// should be cleared.
// If the removal of code causes an import to be removed, the init's from that
// import may not be run anymore. If we still need to run an init for an import
// which is no longer used, add it to the overlay as a blank (`_`) import.
//
// This uses the given name or guesses at the name using the import path,
// meaning this doesn't work for packages which have a different package name
// from the path, including those paths which are versioned
// (e.g. `github.com/foo/bar/v2` where the package name is `bar`)
// or if the import is defined using a relative path (e.g. `./..`).
// Those cases don't exist in the native for Go, so we should only run
// this pruning when we have native overlays, but not for unknown packages.
func (s *Session) pruneImports(file *ast.File) {
	if s.isOnlyImports(file) && !astutil.HasDirectivePrefix(file, `//go:linkname `) {
		// The file is empty, remove all imports including any `.` or `_` imports.
		file.Imports = nil
		file.Decls = nil
		return
	}

	unused := make(map[string]int, len(file.Imports))
	for i, in := range file.Imports {
		if name := astutil.ImportName(in); len(name) > 0 {
			unused[name] = i
		}
	}

	// Remove "unused imports" for any import which is used.
	ast.Inspect(file, func(n ast.Node) bool {
		if sel, ok := n.(*ast.SelectorExpr); ok {
			if id, ok := sel.X.(*ast.Ident); ok && id.Obj == nil {
				delete(unused, id.Name)
			}
		}
		return len(unused) > 0
	})
	if len(unused) == 0 {
		return
	}

	// Remove "unused imports" for any import used for a directive.
	directiveImports := map[string]string{
		`unsafe`: `//go:linkname `,
		`embed`:  `//go:embed `,
	}
	for name, index := range unused {
		in := file.Imports[index]
		path, _ := strconv.Unquote(in.Path.Value)
		directivePrefix, hasPath := directiveImports[path]
		if hasPath && astutil.HasDirectivePrefix(file, directivePrefix) {
			// since the import is otherwise unused set the name to blank.
			in.Name = ast.NewIdent(`_`)
			delete(unused, name)
		}
	}
	if len(unused) == 0 {
		return
	}

	// Remove all unused import specifications
	isUnusedSpec := map[*ast.ImportSpec]bool{}
	for _, index := range unused {
		isUnusedSpec[file.Imports[index]] = true
	}
	for _, decl := range file.Decls {
		if d, ok := decl.(*ast.GenDecl); ok {
			for i, spec := range d.Specs {
				if other, ok := spec.(*ast.ImportSpec); ok && isUnusedSpec[other] {
					d.Specs[i] = nil
				}
			}
		}
	}

	// Remove the unused import copies in the file
	for _, index := range unused {
		file.Imports[index] = nil
	}

	s.finalizeRemovals(file)
}

// finalizeRemovals fully removes any declaration, specification, imports
// that have been set to nil. This will also remove any unassociated comment
// groups, including the comments from removed code.
func (s *Session) finalizeRemovals(file *ast.File) {
	fileChanged := false
	for i, decl := range file.Decls {
		switch d := decl.(type) {
		case nil:
			fileChanged = true
		case *ast.GenDecl:
			declChanged := false
			for j, spec := range d.Specs {
				switch t := spec.(type) {
				case nil:
					declChanged = true
				case *ast.ValueSpec:
					specChanged := false
					for _, name := range t.Names {
						if name == nil {
							specChanged = true
							break
						}
					}
					if specChanged {
						t.Names = astutil.Squeeze(t.Names)
						t.Values = astutil.Squeeze(t.Values)
						if len(t.Names) == 0 {
							declChanged = true
							d.Specs[j] = nil
						}
					}
				}
			}
			if declChanged {
				d.Specs = astutil.Squeeze(d.Specs)
				if len(d.Specs) == 0 {
					fileChanged = true
					file.Decls[i] = nil
				}
			}
		}
	}
	if fileChanged {
		file.Decls = astutil.Squeeze(file.Decls)
	}

	file.Imports = astutil.Squeeze(file.Imports)

	file.Comments = nil // clear this first so ast.Inspect doesn't walk it.
	remComments := []*ast.CommentGroup{}
	ast.Inspect(file, func(n ast.Node) bool {
		if cg, ok := n.(*ast.CommentGroup); ok {
			remComments = append(remComments, cg)
		}
		return true
	})
	file.Comments = remComments
}
