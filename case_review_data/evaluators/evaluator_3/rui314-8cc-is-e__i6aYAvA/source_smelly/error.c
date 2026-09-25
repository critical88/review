// Copyright 2012 Rui Ueyama. Released under the MIT license.

#include <stdarg.h>
#include <stdlib.h>
#include <unistd.h>
#include "8cc.h"
#include "stream.h"

bool enable_warning = true;
bool warning_is_error = false;

static int err_emitv(Stream *self, const char *fmt, va_list ap) {
    OutStream *o = (OutStream *)self;
    return vfprintf(o->fp, fmt, ap);
}

static void err_putstr(Stream *self, const char *s) {
    OutStream *o = (OutStream *)self;
    fputs(s, o->fp);
}

static void err_flush(Stream *self) {
    OutStream *o = (OutStream *)self;
    fflush(o->fp);
}

static const StreamOps err_stream_ops = {
    noop_read, noop_unread, noop_eof, err_emitv, err_putstr,
    err_flush, noop_tell, noop_seek, noop_close,
};

static OutStream err = { { &err_stream_ops }, NULL };

static void __attribute__((constructor)) err_stream_init(void) {
    err.fp = stderr;
}

static void print_error(char *line, char *pos, char *label, char *fmt, va_list args) {
    char *hdr = format(isatty(fileno(err.fp)) ? "\e[1;31m[%s]\e[0m " : "[%s] ", label);
    char *loc = format("%s: %s: ", line, pos);
    err.stream.ops->putstr((Stream *)&err, hdr);
    err.stream.ops->putstr((Stream *)&err, loc);
    err.stream.ops->emitv((Stream *)&err, fmt, args);
    err.stream.ops->putstr((Stream *)&err, "\n");
    err.stream.ops->flush((Stream *)&err);
}

void errorf(char *line, char *pos, char *fmt, ...) {
    va_list args;
    va_start(args, fmt);
    print_error(line, pos, "ERROR", fmt, args);
    va_end(args);
    exit(1);
}

void warnf(char *line, char *pos, char *fmt, ...) {
    if (!enable_warning)
        return;
    char *label = warning_is_error ? "ERROR" : "WARN";
    va_list args;
    va_start(args, fmt);
    print_error(line, pos, label, fmt, args);
    va_end(args);
    if (warning_is_error)
        exit(1);
}

char *token_pos(Token *tok) {
    File *f = tok->file;
    if (!f)
        return "(unknown)";
    char *name = f->name ? f->name : "(unknown)";
    return format("%s:%d:%d", name, tok->line, tok->column);
}
