package installpackage

import (
	"context"
	"fmt"
	"log/slog"

	"github.com/aquaproj/aqua/v2/pkg/config"
	"github.com/aquaproj/aqua/v2/pkg/download"
	"github.com/aquaproj/aqua/v2/pkg/runtime"
	"github.com/aquaproj/aqua/v2/pkg/slsa"
)

type slsaVerifier struct {
	disabled      bool
	pkg           *config.Package
	toolInstaller *DedicatedInstaller
	runtime       *runtime.Runtime
	asset         string
	// installer is the coordinator that owns the asset verification. It is
	// bound when the verifier is created by the installer.
	installer *Installer
}

func (s *slsaVerifier) Enabled(logger *slog.Logger) (bool, error) {
	if s.disabled {
		logger.Debug("slsa verification is disabled")
		return false, nil
	}
	return s.pkg.PackageInfo.SLSAProvenance.GetEnabled(), nil
}

func (s *slsaVerifier) Verify(ctx context.Context, logger *slog.Logger, file string) error {
	return s.installer.verifyArtifactSLSA(ctx, logger, s, file)
}

// verifyArtifactSLSA verifies a downloaded asset with slsa-verifier, installing
// the verifier tool first when it isn't installed yet.
func (is *Installer) verifyArtifactSLSA(ctx context.Context, logger *slog.Logger, s *slsaVerifier, file string) error {
	logger.Info("verify a package with slsa-verifier")
	if err := s.toolInstaller.install(ctx, logger); err != nil {
		return fmt.Errorf("install slsa-verifier: %w", err)
	}

	pkg := s.pkg
	pkgInfo := s.pkg.PackageInfo

	art := pkg.TemplateArtifact(s.runtime, s.asset)
	sourceTag := pkgInfo.SLSAProvenance.SourceTag
	if sourceTag == "" {
		sourceTag = pkg.Package.Version
	}

	if err := is.slsaVerifier.Verify(ctx, logger, s.runtime, pkgInfo.SLSAProvenance, art, &download.File{
		RepoOwner: pkgInfo.RepoOwner,
		RepoName:  pkgInfo.RepoName,
		Version:   pkg.Package.Version,
	}, &slsa.ParamVerify{
		SourceURI:    pkgInfo.SLSASourceURI(),
		SourceTag:    sourceTag,
		ArtifactPath: file,
	}); err != nil {
		return fmt.Errorf("verify a package with slsa-verifier: %w", err)
	}
	return nil
}
