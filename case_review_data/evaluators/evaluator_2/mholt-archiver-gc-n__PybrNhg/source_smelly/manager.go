package archiver

import (
	"context"
	"fmt"
	"io"
	"io/fs"
	"log"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"
)

// ArchiveManager is the package's high-level coordinator. It provides the
// facade that most programs actually want: one value that owns the registry
// of supported formats and performs every common archiving task — snarfing
// files from disk, detecting the format of an archive or compressed file,
// creating archives (optionally compressed), extracting archives to disk, and
// presenting archive contents through the io/fs interfaces.
//
// Because the manager owns its own format registry and its own operational
// policies, embedding programs can create isolated managers that support
// only a subset of the formats (or their own custom formats) instead of
// relying on package-global state. All package-level convenience functions
// (Identify, FileSystem, FilesFromDisk, and RegisterFormat) delegate to
// DefaultArchiveManager, which is pre-registered with every built-in format.
type ArchiveManager struct {
	// formats is the registry of formats this manager supports, keyed by
	// the format extension without the leading dot.
	formats map[string]Format

	// FollowSymlinks and ClearAttributes are the on-disk gathering
	// defaults used when no FromDiskOptions are supplied. They mirror
	// the options of the same names; see FromDiskOptions.
	FollowSymlinks  bool
	ClearAttributes bool

	// ImplicitTopLevelFolder causes ArchiveFolder to nest the contents of
	// a single gathered folder into a folder of the same name inside the
	// archive, so that extracting the archive yields that folder.
	ImplicitTopLevelFolder bool

	// OverwriteExisting allows ExtractToFolder to replace files that
	// already exist on disk; by default existing files are preserved and
	// the extraction of that file fails.
	OverwriteExisting bool

	// ContinueOnError makes extraction to a folder log (as opposed to
	// fail on) individual file write errors and continue with the rest
	// of the archive.
	ContinueOnError bool

	// Prefix is the subdirectory that returned virtual file systems are
	// rooted in by default; it mirrors ArchiveFS.Prefix. An empty Prefix
	// roots file systems at the archive root, which preserves the
	// historical behavior.
	Prefix string
}

// NewArchiveManager returns a new manager with an empty format registry.
// Register the desired formats (see RegisterFormat) before calling its
// archival, extraction, or identification operations; or use
// DefaultArchiveManager, which recognizes all the built-in formats.
func NewArchiveManager() ArchiveManager {
	return ArchiveManager{formats: make(map[string]Format)}
}

// DefaultArchiveManager is the manager used by the package-level
// convenience functions. It is pre-registered with every built-in format
// during package initialization (see RegisterFormat) and carries the
// default, backward-compatible policies.
var DefaultArchiveManager = NewArchiveManager()

// SupportedFormats returns the names (extensions without the leading dot)
// of every format registered with this manager, in sorted order.
func (m ArchiveManager) SupportedFormats() []string {
	var names []string
	for name := range m.formats {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

// ByExtension returns the format whose extension matches the end of
// filename, looking only at the formats registered with this manager.
// Archive files wrapped in a registered compression format (for example
// ".tar.gz") resolve to the combined Archive format, so that the returned
// Format value can do everything the file name implies.
func (m ArchiveManager) ByExtension(filename string) (Format, error) {
	lower := strings.ToLower(filename)

	// peel registered compression extensions off the end of the file name
	// so that compressed archive names (".tar.gz", ".tar.bz2", ...) surface
	// the archive format underneath them
	var compression Compression
	for {
		var peeled bool
		for _, format := range m.formats {
			cf, isCompression := format.(Compression)
			if !isCompression || !strings.HasSuffix(lower, strings.ToLower(format.Extension())) {
				continue
			}
			if compression == nil {
				compression = cf
			}
			lower = strings.TrimSuffix(lower, strings.ToLower(format.Extension()))
			peeled = true
		}
		if !peeled {
			break
		}
	}

	// then match the archive format, if any, that the name ends with
	var archival Archival
	var extraction Extraction
	for _, format := range m.formats {
		ar, isArchive := format.(Archival)
		ex, isExtract := format.(Extraction)
		if (isArchive || isExtract) && strings.HasSuffix(lower, strings.ToLower(format.Extension())) {
			archival, extraction = ar, ex
			break
		}
	}

	switch {
	case compression != nil && archival != nil:
		return Archive{compression, archival, extraction}, nil
	case compression != nil:
		return compression, nil
	case archival != nil:
		return archival, nil
	default:
		return nil, fmt.Errorf("no registered format matches the extension of %s", filename)
	}
}

// ArchiveFolder creates a new archive at destFilename, gathering the input
// files by walking filenames on disk (in the same way as FilesFromDisk; the
// manager's on-disk gathering defaults apply). The archive format (including
// any compression, as in ".tar.gz") is chosen by matching destFilename
// against the formats registered with this manager. With
// ImplicitTopLevelFolder set, archiving a single folder nest its contents
// into a same-named folder so that extraction re-creates it.
func (m ArchiveManager) ArchiveFolder(ctx context.Context, destFilename string, filenames map[string]string) error {
	if len(filenames) == 0 {
		return fmt.Errorf("no filenames provided to archive")
	}

	// choose the destination format from the file name using only the
	// formats this manager knows about
	format, err := m.ByExtension(destFilename)
	if err != nil {
		return fmt.Errorf("choosing archive format for %s: %w", destFilename, err)
	}
	archival, isArchival := format.(Archival)
	if !isArchival {
		return fmt.Errorf("registered format %s (%T) cannot create archives", format.Extension(), format)
	}

	// gather the files to archive, honoring this manager's on-disk defaults
	files, err := m.FilesFromDisk(nil, filenames)
	if err != nil {
		return fmt.Errorf("gathering files: %w", err)
	}

	// optionally nest single-folder contents into a same-named folder
	if m.ImplicitTopLevelFolder && len(filenames) == 1 {
		for root := range filenames {
			folder := filepath.Base(root)
			for i := range files {
				files[i].NameInArchive = path.Join(folder, files[i].NameInArchive)
			}
		}
	}

	out, err := os.Create(destFilename)
	if err != nil {
		return err
	}

	err = archival.Archive(ctx, out, files)
	if closeErr := out.Close(); err == nil {
		err = closeErr
	}
	return err
}

// ExtractToFolder extracts sourceArchive — any archive or compressed
// archive whose format is registered with this manager — writing each
// entry into destDir on the local disk, creating parent folders as
// needed. Entries are written at their names within the archive and never
// escape destDir. Existing files are preserved (see OverwriteExisting),
// and with ContinueOnError set, entries that fail to write are logged
// and the walk carries on.
func (m ArchiveManager) ExtractToFolder(ctx context.Context, sourceArchive io.Reader, destDir string) error {
	if sourceArchive == nil {
		return fmt.Errorf("no archive input given")
	}

	// detect the archive format from the stream contents using this
	// manager's registry
	format, stream, err := m.Identify(ctx, "", sourceArchive)
	if err != nil {
		return fmt.Errorf("identifying archive: %w", err)
	}
	extraction, isExtraction := format.(Extractor)
	if !isExtraction {
		return fmt.Errorf("registered format %s (%T) cannot extract archives", format.Extension(), format)
	}

	return extraction.Extract(ctx, stream, func(ctx context.Context, file FileInfo) error {
		if err := ctx.Err(); err != nil {
			return err
		}
		if err := m.writeFileToDisk(ctx, file, destDir); err != nil {
			if !m.ContinueOnError {
				return err
			}
			log.Printf("[ERROR] %v", err)
		}
		return nil
	})
}

// writeFileToDisk writes file, encountered during an extraction, into
// destDir on the local disk: directories (including implicit parents) are
// created, links are re-created as links, and regular file bodies are
// copied to their entries' names.
func (m ArchiveManager) writeFileToDisk(ctx context.Context, file FileInfo, destDir string) error {
	if err := ctx.Err(); err != nil {
		return err
	}

	destPath := m.destinationPath(destDir, file.NameInArchive)

	mode := file.Mode()
	perm := mode.Perm()
	if perm == 0 {
		perm = 0o755
	}

	if file.IsDir() {
		if err := os.MkdirAll(destPath, perm); err != nil {
			return err
		}
		return nil
	}

	if mode&fs.ModeSymlink != 0 {
		if err := os.MkdirAll(filepath.Dir(destPath), 0o755); err != nil {
			return err
		}
		return os.Symlink(file.LinkTarget, destPath)
	}

	src, err := file.Open()
	if err != nil {
		return err
	}
	defer src.Close()

	if err := os.MkdirAll(filepath.Dir(destPath), 0o755); err != nil {
		return err
	}

	openFlags := os.O_WRONLY | os.O_CREATE
	if m.OverwriteExisting {
		openFlags |= os.O_TRUNC
	} else {
		openFlags |= os.O_EXCL // refuse to clobber what's already on disk
	}
	dst, err := os.OpenFile(destPath, openFlags, perm)
	if err != nil {
		return err
	}

	if _, err := io.Copy(dst, src); err != nil {
		dst.Close()
		return err
	}
	return dst.Close()
}

// destinationPath joins destDir with the archive-relative name of an entry,
// neutralizing absolute paths and directory traversal so that extraction
// always lands inside destDir.
func (m ArchiveManager) destinationPath(destDir, nameInArchive string) string {
	// prefix with "/" so the path is cleaned as absolute, which resolves
	// any ".." components harmlessly instead of escaping the destination
	cleaned := strings.TrimPrefix(path.Clean(path.Join("/", nameInArchive)), "/")
	return filepath.Join(destDir, filepath.FromSlash(cleaned))
}
