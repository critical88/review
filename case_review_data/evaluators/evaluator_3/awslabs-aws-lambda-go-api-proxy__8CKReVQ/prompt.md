# Refactor: consolidate repeated conversion-parameter groups in the event pipeline

Repository: `github.com/awslabs/aws-lambda-go-api-proxy` (Go).

## Area

The `core` package converts AWS proxy events into `*http.Request` objects and converts `http.ResponseWriter` state back into gateway responses. It handles three event flavors — REST API v1, HTTP API v2, and ALB target-group requests — and a symmetric response writer for each flavor. Conversion flows through the same lifecycle stages in every flavor: decode the request body, normalize the event path and strip the configured base path, resolve the custom host, assemble the query string, build the `*http.Request`, and propagate the event headers; on response, encode the body and assemble the gateway response from status, headers, body, and the base64 flag.

## Observation

A maintainer recently extracted each conversion stage into named per-flavor helper methods so each stage is readable and independently documented. As a side effect, the same conceptual conversion data is now passed as the same long parameter list through the corresponding helper signature in each flavor:

- the request-assembly inputs — the HTTP method, the request URL, the decoded body, and the raw event method and path — are passed as separate parameters through the request-construction helper of each request flavor;
- the response-assembly inputs — status, headers, the encoded body, and the base64 flag — are passed as separate parameters through the response-construction helper of each response flavor;
- the query-string inputs and the header-propagation inputs are likewise passed as separate parameters through the matching helpers where the flavors share that stage.

The same data therefore travels as separate parameters through multiple signatures instead of being carried by one abstraction. When an assembly input changes, the parameter list has to be updated in several places, and adding another event flavor would mean copying the same parameter list yet again.

## Goal

Consolidate the recurring parameter groups into well-designed shared abstractions so each conversion stage that is genuinely shared across flavors is expressed once, carrying the same conceptual data in a single abstraction rather than threading it as separate parameters through every signature. Identify every instance of these recurring parameter groups across the three request flavors and the three response flavors, and replace each with an appropriate abstraction — for example a parameter object or typed record that groups the values a stage operates on, or a shared helper that owns that stage — chosen to fit this codebase. The code you write must compile and all existing tests must continue to pass.

## What behavior and API must remain stable

- The repository builds and the full test suite passes.
- The three event flavors produce identical `*http.Request` objects and identical gateway responses for any given event.
- Method uppercasing, base-path stripping, custom-host resolution, query-string assembly, header propagation (singleton-header handling, comma-splitting for multi-value headers, cookie appending for the v2 flavor), and base64 body decoding are unchanged.
- Error messages are preserved exactly, including the different capitalization used by the API Gateway flavors and the ALB flavor.
- The `GO_API_HOST` override is honored for the API Gateway flavors and is not honored for the ALB flavor, exactly as today.
- The public API of the `core` package and every adapter package is unchanged; the thin adapter wrappers that delegate to `core` must continue to compile.
