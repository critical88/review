package installpackage

import (
	"context"
	"fmt"
	"log/slog"

	"github.com/aquaproj/aqua/v2/pkg/config"
	"github.com/aquaproj/aqua/v2/pkg/config/registry"
	"github.com/aquaproj/aqua/v2/pkg/ghattestation"
)

type FileVerifier interface {
	Enabled(logger *slog.Logger) (bool, error)
	Verify(ctx context.Context, logger *slog.Logger, file string) error
}

type gitHubArtifactAttestationsVerifier struct {
	disabled    bool
	gaa         *registry.GitHubArtifactAttestations
	pkg         *config.Package
	ghInstaller *DedicatedInstaller
	// installer is the coordinator that owns the asset verification. It is
	// bound when the verifier is created by the installer.
	installer *Installer
}

func (g *gitHubArtifactAttestationsVerifier) Enabled(logger *slog.Logger) (bool, error) {
	if g.disabled {
		logger.Debug("GitHub Artifact Attestation is disabled")
		return false, nil
	}
	return g.gaa.GetEnabled(), nil
}

func (g *gitHubArtifactAttestationsVerifier) Verify(ctx context.Context, logger *slog.Logger, file string) error {
	return g.installer.verifyArtifactGitHubAttestations(ctx, logger, g, file)
}

// verifyArtifactGitHubAttestations verifies a downloaded asset with GitHub
// Artifact Attestations, installing GitHub CLI first when it isn't installed
// yet.
func (is *Installer) verifyArtifactGitHubAttestations(ctx context.Context, logger *slog.Logger, g *gitHubArtifactAttestationsVerifier, file string) error {
	logger.Info("verify GitHub Artifact Attestations")
	if err := g.ghInstaller.install(ctx, logger); err != nil {
		return fmt.Errorf("install GitHub CLI: %w", err)
	}

	if err := is.ghVerifier.Verify(ctx, logger, &ghattestation.ParamVerify{
		Repository:     g.pkg.PackageInfo.RepoOwner + "/" + g.pkg.PackageInfo.RepoName,
		ArtifactPath:   file,
		PredicateType:  g.gaa.PredicateType,
		SignerWorkflow: g.gaa.SignerWorkflow(),
	}); err != nil {
		return fmt.Errorf("verify a package with gh attestation: %w", err)
	}
	return nil
}
