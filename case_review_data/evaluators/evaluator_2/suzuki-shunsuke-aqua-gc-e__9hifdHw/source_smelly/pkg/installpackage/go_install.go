package installpackage

import (
	"context"
	"fmt"
	"log/slog"
	"os"

	"github.com/aquaproj/aqua/v2/pkg/config"
	"github.com/aquaproj/aqua/v2/pkg/osexec"
)

type GoInstallInstaller interface {
	Install(ctx context.Context, path, gobin string) error
}

type GoInstallInstallerImpl struct {
	exec Executor
	// installer is the coordinator that owns the module installation. It is
	// bound by the installer's constructor so the command runs with the
	// installer's own runtime configuration.
	installer *Installer
}

func NewGoInstallInstallerImpl(exec Executor) *GoInstallInstallerImpl {
	return &GoInstallInstallerImpl{
		exec: exec,
	}
}

func (is *GoInstallInstallerImpl) Install(ctx context.Context, path, gobin string) error {
	return is.installer.runGoInstall(ctx, path, gobin)
}

// runGoInstall installs a Go module into the given GOBIN directory.
func (is *Installer) runGoInstall(ctx context.Context, path, gobin string) error {
	cmd := osexec.Command(ctx, "go", "install", path)
	cmd.Env = append(os.Environ(), "GOBIN="+gobin)
	if _, err := is.exec.ExecStderr(cmd); err != nil {
		return fmt.Errorf("install a go package: %w", err)
	}
	return nil
}

func (is *Installer) downloadGoInstall(ctx context.Context, logger *slog.Logger, pkg *config.Package, dest string) error {
	p, err := pkg.RenderPath()
	if err != nil {
		return fmt.Errorf("render Go Module Path: %w", err)
	}
	goPkgPath := p + "@" + pkg.Package.Version
	logger.Info("Installing a Go tool",
		"gobin", dest,
		"go_package_path", goPkgPath)
	if err := is.goInstallInstaller.Install(ctx, goPkgPath, dest); err != nil {
		return fmt.Errorf("build Go tool: %w", err)
	}
	return nil
}
