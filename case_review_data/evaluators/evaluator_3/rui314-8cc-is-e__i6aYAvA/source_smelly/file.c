// Copyright 2014 Rui Ueyama. Released under the MIT license.

/*
 * This file provides character input stream for C source code.
 * An input stream is either backed by stdio's FILE * or
 * backed by a string.
 * The following input processing is done at this stage.
 *
 * - C11 5.1.1.2p1: "\r\n" or "\r" are canonicalized to "\n".
 * - C11 5.1.1.2p2: A sequence of backslash and newline is removed.
 * - EOF not immediately following a newline is converted to
 *   a sequence of newline and EOF. (The C spec requires source
 *   files end in a newline character (5.1.1.2p2). Thus, if all
 *   source files are comforming, this step wouldn't be needed.)
 *
 * Trigraphs are not supported by design.
 */

#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>
#include "8cc.h"
#include "stream.h"

static Vector *files = &EMPTY_VECTOR;
static Vector *stashed = &EMPTY_VECTOR;

static const StreamOps file_stream_ops;
static const StreamOps string_stream_ops;

File *make_file(FILE *file, char *name) {
    File *r = calloc(1, sizeof(File));
    r->stream.ops = &file_stream_ops;
    r->file = file;
    r->name = name;
    r->line = 1;
    r->column = 1;
    struct stat st;
    if (fstat(fileno(file), &st) == -1)
        error("fstat failed: %s", strerror(errno));
    r->mtime = st.st_mtime;
    return r;
}

File *make_file_string(char *s) {
    File *r = calloc(1, sizeof(File));
    r->stream.ops = &string_stream_ops;
    r->line = 1;
    r->column = 1;
    r->p = s;
    return r;
}

static void file_close(Stream *self) {
    File *f = (File *)self;
    if (f->file)
        fclose(f->file);
}

static int file_read(Stream *self) {
    File *f = (File *)self;
    int c = getc(f->file);
    if (c == EOF) {
        c = (f->last == '\n' || f->last == EOF) ? EOF : '\n';
    } else if (c == '\r') {
        int c2 = getc(f->file);
        if (c2 != '\n')
            ungetc(c2, f->file);
        c = '\n';
    }
    f->last = c;
    return c;
}

static int string_read(Stream *self) {
    File *f = (File *)self;
    int c;
    if (*f->p == '\0') {
        c = (f->last == '\n' || f->last == EOF) ? EOF : '\n';
    } else if (*f->p == '\r') {
        f->p++;
        if (*f->p == '\n')
            f->p++;
        c = '\n';
    } else {
        c = *f->p++;
    }
    f->last = c;
    return c;
}

static void stream_unread(Stream *self, int c) {
    File *f = (File *)self;
    assert(f->buflen < sizeof(f->buf) / sizeof(f->buf[0]));
    f->buf[f->buflen++] = c;
    if (c == '\n') {
        f->column = 1;
        f->line--;
    } else {
        f->column--;
    }
}

static bool file_eof(Stream *self) {
    File *f = (File *)self;
    return f->last == EOF && f->buflen == 0;
}

static bool string_eof(Stream *self) {
    File *f = (File *)self;
    return *f->p == '\0' && f->buflen == 0;
}

static const StreamOps file_stream_ops = {
    file_read, stream_unread, file_eof, noop_emitv, noop_putstr,
    noop_flush, noop_tell, noop_seek, file_close,
};

static const StreamOps string_stream_ops = {
    string_read, stream_unread, string_eof, noop_emitv, noop_putstr,
    noop_flush, noop_tell, noop_seek, file_close,
};

static int get() {
    File *f = vec_tail(files);
    int c;
    if (f->buflen > 0) {
        c = f->buf[--f->buflen];
    } else {
        c = f->stream.ops->read((Stream *)f);
    }
    if (c == '\n') {
        f->line++;
        f->column = 1;
    } else if (c != EOF) {
        f->column++;
    }
    return c;
}

int readc() {
    for (;;) {
        int c = get();
        if (c == EOF) {
            if (vec_len(files) == 1)
                return c;
            File *g = vec_pop(files);
            g->stream.ops->close((Stream *)g);
            continue;
        }
        if (c != '\\')
            return c;
        int c2 = get();
        if (c2 == '\n')
            continue;
        unreadc(c2);
        return c;
    }
}

void unreadc(int c) {
    if (c == EOF)
        return;
    File *f = vec_tail(files);
    f->stream.ops->unread((Stream *)f, c);
}

File *current_file() {
    return vec_tail(files);
}

void stream_push(File *f) {
    vec_push(files, f);
}

int stream_depth() {
    return vec_len(files);
}

char *input_position() {
    if (vec_len(files) == 0)
        return "(unknown)";
    File *f = vec_tail(files);
    return format("%s:%d:%d", f->name, f->line, f->column);
}

void stream_stash(File *f) {
    vec_push(stashed, files);
    files = make_vector1(f);
}

void stream_unstash() {
    files = vec_pop(stashed);
}
