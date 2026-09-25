package installpackage

import (
	"context"
	"fmt"
	"log/slog"

	"github.com/aquaproj/aqua/v2/pkg/config"
	"github.com/aquaproj/aqua/v2/pkg/config/registry"
	"github.com/aquaproj/aqua/v2/pkg/download"
	"github.com/aquaproj/aqua/v2/pkg/runtime"
)

type cosignVerifier struct {
	disabled      bool
	pkg           *config.Package
	cosign        *registry.Cosign
	toolInstaller *DedicatedInstaller
	runtime       *runtime.Runtime
	asset         string
	// installer is the coordinator that owns the asset verification. It is
	// bound when the verifier is created by the installer.
	installer *Installer
}

func (c *cosignVerifier) Enabled(logger *slog.Logger) (bool, error) {
	if c.disabled {
		logger.Debug("cosign is disabled")
		return false, nil
	}

	return c.cosign.GetEnabled(), nil
}

func (c *cosignVerifier) Verify(ctx context.Context, logger *slog.Logger, file string) error {
	return c.installer.verifyArtifactCosign(ctx, logger, c, file)
}

// verifyArtifactCosign verifies a downloaded asset with Cosign, installing
// cosign first when it isn't installed yet.
func (is *Installer) verifyArtifactCosign(ctx context.Context, logger *slog.Logger, c *cosignVerifier, file string) error {
	logger.Info("verifying a file with Cosign")
	if err := c.toolInstaller.install(ctx, logger); err != nil {
		return fmt.Errorf("install sigstore/cosign: %w", err)
	}

	pkg := c.pkg
	cos := c.cosign

	art := pkg.TemplateArtifact(c.runtime, c.asset)

	if err := is.cosign.Verify(ctx, logger, c.runtime, &download.File{
		RepoOwner: pkg.PackageInfo.RepoOwner,
		RepoName:  pkg.PackageInfo.RepoName,
		Version:   pkg.Package.Version,
	}, cos, art, file); err != nil {
		return fmt.Errorf("verify a file with Cosign: %w", err)
	}
	return nil
}
