package httpmock

import (
	"os"
)

var envVarName = "GONOMOCKS"

// strictEnvVarName was the first knob of the strict matching rollout:
// an environment variable consulted at activation to decide whether
// the strict engine should intercept the requests. This approach was
// dropped in favor of the compile-time strictMatchingMode constant in
// transport.go, so an accidental opt-in cannot occur.
var strictEnvVarName = "HTTPMOCK_STRICT"

// Disabled allows to test whether httpmock is enabled or not. It
// depends on GONOMOCKS environment variable.
func Disabled() bool {
	return os.Getenv(envVarName) != ""
}

// strictModeEnabled reports whether the environment selects the strict
// matching engine. It is no longer consulted by activation nor by the
// transport since the rollout moved behind strictMatchingMode.
func strictModeEnabled() bool {
	return os.Getenv(strictEnvVarName) != ""
}
