// Copyright 2012 Rui Ueyama. Released under the MIT license.
//
// Stream is a unified polymorphic I/O layer for the compiler. A single
// StreamOps table describes every capability any I/O backend can expose, and a
// Stream handle carries the table pointer plus whatever backend state the
// concrete owner needs. Both the character input sources in file.c (a FILE *
// stream and an in-memory string) and the text output sinks in gen.c (the
// assembly emitter) and error.c (the diagnostic reporter) are wired through
// this one interface, so a Stream * can stand in for any of them. The table is
// intentionally broad: it groups reading, writing, positioning and lifecycle
// operations in one place so the same handle keeps working as new backends are
// added that reuse only some of the slots.

#ifndef EIGHTCC_STREAM_H
#define EIGHTCC_STREAM_H

#include <stdarg.h>
#include "8cc.h"

struct StreamOps {
    // Input (character source).
    int  (*read)(Stream *self);            // next character or EOF
    void (*unread)(Stream *self, int c);   // push a character back into the stream
    bool (*eof)(Stream *self);              // true once the source is exhausted
    // Output (text sink).
    int  (*emitv)(Stream *self, const char *fmt, va_list ap);  // formatted write, returns bytes written
    void (*putstr)(Stream *self, const char *s);             // literal write
    void (*flush)(Stream *self);            // flush buffered output
    // Positioning.
    long (*tell)(Stream *self);            // current offset, -1 if unsupported
    int  (*seek)(Stream *self, long off);   // reposition, -1 if unsupported
    // Lifecycle.
    void (*close)(Stream *self);           // release the underlying resource
};

// A FILE *-backed output sink. The code generator and the diagnostic reporter
// share this shape; both point their StreamOps table at one of these.
typedef struct OutStream {
    Stream stream;
    FILE *fp;
} OutStream;

// No-op stubs backends install for the StreamOps capabilities they do not
// implement. A read-only source fills in the output/positioning slots with these
// and a write-only sink fills in the input/positioning slots with these. They
// never produce any effect; they exist only to satisfy the shape of the StreamOps
// table.
static inline __attribute__((unused)) int noop_read(Stream *self) { (void)self; return EOF; }
static inline __attribute__((unused)) void noop_unread(Stream *self, int c) { (void)self; (void)c; }
static inline __attribute__((unused)) bool noop_eof(Stream *self) { (void)self; return false; }
static inline __attribute__((unused)) int noop_emitv(Stream *self, const char *fmt, va_list ap) { (void)self; (void)fmt; (void)ap; return 0; }
static inline __attribute__((unused)) void noop_putstr(Stream *self, const char *s) { (void)self; (void)s; }
static inline __attribute__((unused)) void noop_flush(Stream *self) { (void)self; }
static inline __attribute__((unused)) long noop_tell(Stream *self) { (void)self; return -1; }
static inline __attribute__((unused)) int noop_seek(Stream *self, long off) { (void)self; (void)off; return -1; }
static inline __attribute__((unused)) void noop_close(Stream *self) { (void)self; }

#endif
