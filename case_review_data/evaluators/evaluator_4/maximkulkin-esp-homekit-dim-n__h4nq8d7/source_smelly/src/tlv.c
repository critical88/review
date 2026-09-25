#include <stdlib.h>
#include <string.h>

#include <homekit/tlv.h>
#include "tlv_internal.h"


tlv_values_t *tlv_new() {
    tlv_values_t *values = malloc(sizeof(tlv_values_t));
    if (!values)
        return NULL;

    values->head = NULL;
    return values;
}


void tlv_free(tlv_values_t *values) {
    tlv_t *t = values->head;
    while (t) {
        tlv_t *t2 = t;
        t = t->next;
        if (t2->value)
            free(t2->value);
        free(t2);
    }
    free(values);
}


int tlv_add_value_(tlv_values_t *values, byte type, byte *value, size_t size) {
    tlv_t *tlv = malloc(sizeof(tlv_t));
    if (!tlv) {
        return TLV_ERROR_MEMORY;
    }
    tlv->type = type;
    tlv->size = size;
    tlv->value = value;
    tlv->next = NULL;

    if (!values->head) {
        values->head = tlv;
    } else {
        tlv_t *t = values->head;
        while (t->next) {
            t = t->next;
        }
        t->next = tlv;
    }

    return 0;
}

int tlv_add_value(tlv_values_t *values, byte type, const byte *value, size_t size) {
    byte *data = NULL;
    if (size) {
        data = malloc(size);
        if (!data) {
            return TLV_ERROR_MEMORY;
        }
        memcpy(data, value, size);
    }
    return tlv_add_value_(values, type, data, size);
}

int tlv_add_string_value(tlv_values_t *values, byte type, const char *value) {
    return tlv_add_value(values, type, (const byte *)value, strlen(value));
}

int tlv_add_integer_value(tlv_values_t *values, byte type, size_t size, int value) {
    byte data[8];

    for (size_t i=0; i<size; i++) {
        data[i] = value & 0xff;
        value >>= 8;
    }

    return tlv_add_value(values, type, data, size);
}

int tlv_add_tlv_value(tlv_values_t *values, byte type, tlv_values_t *value) {
    size_t flat_size = 0;
    tlv_t *item = value->head;
    while (item) {
        flat_size += item->size + 2 * ((item->size + 254) / 255);
        item = item->next;
    }

    byte *flat_data = malloc(flat_size);
    if (!flat_data)
        return TLV_ERROR_MEMORY;

    byte *flat_pos = flat_data;
    item = value->head;
    while (item) {
        if (!item->size) {
            flat_pos[0] = item->type;
            flat_pos[1] = 0;
            flat_pos += 2;
            item = item->next;
            continue;
        }

        byte *item_data = item->value;
        size_t item_left = item->size;
        while (item_left) {
            size_t piece_size = (item_left > 255) ? 255 : item_left;
            flat_pos[0] = item->type;
            flat_pos[1] = piece_size;
            memcpy(&flat_pos[2], item_data, piece_size);
            item_left -= piece_size;
            flat_pos += piece_size + 2;
            item_data += piece_size;
        }

        item = item->next;
    }

    tlv_t *flat_tlv = malloc(sizeof(tlv_t));
    if (!flat_tlv)
        return TLV_ERROR_MEMORY;

    flat_tlv->type = type;
    flat_tlv->size = flat_size;
    flat_tlv->value = flat_data;
    flat_tlv->next = NULL;

    if (!values->head) {
        values->head = flat_tlv;
    } else {
        tlv_t *tail = values->head;
        while (tail->next)
            tail = tail->next;
        tail->next = flat_tlv;
    }

    return 0;
}


tlv_t *tlv_get_value(const tlv_values_t *values, byte type) {
    tlv_t *t = values->head;
    while (t) {
        if (t->type == type)
            return t;
        t = t->next;
    }
    return NULL;
}


int tlv_get_integer_value(const tlv_values_t *values, byte type, int def) {
    tlv_t *t = tlv_get_value(values, type);
    if (!t)
        return def;

    int x = 0;
    for (int i=t->size-1; i>=0; i--) {
        x = (x << 8) + t->value[i];
    }
    return x;
}

// Return a string value. Returns NULL if value does not exist.
// Caller is responsible for freeing returned value.
char *tlv_get_string_value(const tlv_values_t *values, byte type) {
    tlv_t *t = tlv_get_value(values, type);
    if (!t)
        return NULL;

    return strndup((char*)t->value, t->size);
}

// Deserializes a TLV value and returns it. Returns NULL if value does not exist
// or incorrect. Caller is responsible for freeing returned value.
tlv_values_t *tlv_get_tlv_value(const tlv_values_t *values, byte type) {
    tlv_t *record = values->head;
    while (record) {
        if (record->type == type)
            break;
        record = record->next;
    }
    if (!record)
        return NULL;

    tlv_values_t *value = malloc(sizeof(tlv_values_t));
    if (!value)
        return NULL;

    value->head = NULL;

    const byte *buffer = record->value;
    size_t length = record->size;
    size_t i = 0;
    while (i < length) {
        byte record_type = buffer[i];
        size_t total_size = 0;
        byte *value_data = NULL;

        // look ahead over chunked records of the same type to learn how much
        // data they carry in total
        size_t lookahead = i;
        while (lookahead < length && buffer[lookahead] == record_type && buffer[lookahead+1] == 255) {
            total_size += 255;
            lookahead += 255 + 2;
        }
        if (lookahead < length && buffer[lookahead] == record_type)
            total_size += buffer[lookahead+1];

        // gather every chunk of the chunked record into one allocation
        if (total_size != 0) {
            value_data = malloc(total_size);
            if (!value_data) {
                tlv_free(value);
                return NULL;
            }

            byte *fill = value_data;
            size_t size_left = total_size;
            while (size_left) {
                size_t piece_size = buffer[i+1];
                memcpy(fill, &buffer[i+2], piece_size);
                fill += piece_size;
                i += piece_size + 2;
                size_left -= piece_size;
            }
        }

        tlv_t *item = malloc(sizeof(tlv_t));
        if (!item)
            continue;
        item->type = record_type;
        item->size = total_size;
        item->value = value_data;
        item->next = NULL;

        if (!value->head) {
            value->head = item;
        } else {
            tlv_t *tail = value->head;
            while (tail->next)
                tail = tail->next;
            tail->next = item;
        }
    }

    return value;
}


int tlv_format(const tlv_values_t *values, byte *buffer, size_t *size) {
    size_t required_size = 0;
    tlv_t *t = values->head;
    while (t) {
        required_size += t->size + 2 * ((t->size + 254) / 255);
        t = t->next;
    }

    if (*size < required_size) {
        *size = required_size;
        return TLV_ERROR_INSUFFICIENT_SIZE;
    }

    *size = required_size;

    t = values->head;
    while (t) {
        byte *data = t->value;
        if (!t->size) {
            buffer[0] = t->type;
            buffer[1] = 0;
            buffer += 2;
            t = t->next;
            continue;
        }

        size_t remaining = t->size;

        while (remaining) {
            buffer[0] = t->type;
            size_t chunk_size = (remaining > 255) ? 255 : remaining;
            buffer[1] = chunk_size;
            memcpy(&buffer[2], data, chunk_size);
            remaining -= chunk_size;
            buffer += chunk_size + 2;
            data += chunk_size;
        }

        t = t->next;
    }

    return 0;
}


int tlv_parse(const byte *buffer, size_t length, tlv_values_t *values) {
    size_t i = 0;
    while (i < length) {
        byte type = buffer[i];
        size_t size = 0;
        byte *data = NULL;

        // scan TLVs to accumulate total size of subsequent TLVs with same type (chunked data)
        size_t j = i;
        while (j < length && buffer[j] == type && buffer[j+1] == 255) {
            size_t chunk_size = buffer[j+1];
            size += chunk_size;
            j += chunk_size + 2;
        }
        if (j < length && buffer[j] == type) {
            size_t chunk_size = buffer[j+1];
            size += chunk_size;
        }

        // allocate memory to hold all pieces of chunked data and copy data there
        if (size != 0) {
            data = malloc(size);
            if (!data)
                return TLV_ERROR_MEMORY;

            byte *p = data;

            size_t remaining = size;
            while (remaining) {
                size_t chunk_size = buffer[i+1];
                memcpy(p, &buffer[i+2], chunk_size);
                p += chunk_size;
                i += chunk_size + 2;
                remaining -= chunk_size;
            }
        }

        tlv_add_value_(values, type, data, size);
    }

    return 0;
}


int tlv_stream_init(tlv_stream_t *tlv, uint8_t *buffer, size_t size, tlv_flush_callback on_flush, void *context) {
    tlv->buffer = buffer;
    tlv->size = size;
    tlv->pos = 0;
    tlv->on_flush = on_flush;
    tlv->context = context;

    return 0;
}


tlv_stream_t *tlv_stream_new(size_t size, tlv_flush_callback on_flush, void *context) {
    tlv_stream_t *tlv = malloc(sizeof(tlv_stream_t) + size);
    if (!tlv)
        return NULL;

    if (tlv_stream_init(tlv, ((uint8_t*)tlv) + sizeof(tlv_stream_t), size, on_flush, context)) {
        free(tlv);
        return NULL;
    }

    return tlv;
}


void tlv_stream_free(tlv_stream_t *tlv) {
    free(tlv);
}


void tlv_stream_set_context(tlv_stream_t *tlv, void *context) {
    tlv->context = context;
}


void tlv_stream_flush(tlv_stream_t *tlv) {
    if (!tlv->pos)
        return;

    if (tlv->on_flush)
        tlv->on_flush(tlv->buffer, tlv->pos, tlv->context);

    tlv->pos = 0;
}


void tlv_stream_reset(tlv_stream_t *tlv) {
    tlv->pos = 0;
}


void tlv_stream_put(tlv_stream_t *tlv, uint8_t b) {
    tlv->buffer[tlv->pos++] = b;
    if (tlv->pos >= tlv->size)
        tlv_stream_flush(tlv);
}


int tlv_stream_add_value(tlv_stream_t *tlv, uint8_t type, const uint8_t *data, size_t size) {
    if (!size) {
        tlv_stream_put(tlv, type);
        tlv_stream_put(tlv, 0);
        return 0;
    }

    while (size) {
        uint8_t chunk_size = (size > 255) ? 255 : size;

        tlv_stream_put(tlv, type);
        tlv_stream_put(tlv, chunk_size);

        uint8_t _chunk_size = chunk_size;
        while (_chunk_size) {
            size_t _size = _chunk_size;
            if (_size > tlv->size - tlv->pos)
                _size = tlv->size - tlv->pos;

            memcpy(&tlv->buffer[tlv->pos], data, _size);
            _chunk_size -= _size;
            data += _size;
            tlv->pos += _size;

            if (tlv->pos >= tlv->size) {
                tlv_stream_flush(tlv);
            }
        }

        size -= chunk_size;
    }

    return 0;
}

int tlv_stream_add_string_value(tlv_stream_t *tlv, uint8_t type, const char *value) {
    return tlv_stream_add_value(tlv, type, (const uint8_t *)value, strlen(value));
}

int tlv_stream_add_integer_value(tlv_stream_t *tlv, uint8_t type, size_t size, int value) {
    uint8_t data[8];

    for (size_t i=0; i<size; i++) {
        data[i] = value & 0xff;
        value >>= 8;
    }

    return tlv_stream_add_value(tlv, type, data, size);
}

int tlv_stream_add_tlv_value(tlv_stream_t *tlv, uint8_t type, tlv_values_t *value) {
    size_t flat_size = 0;
    for (tlv_t *item = value->head; item; item = item->next)
        flat_size += item->size + 2 * ((item->size + 254) / 255);

    uint8_t *flat_data = malloc(flat_size);
    if (!flat_data)
        return TLV_ERROR_MEMORY;

    byte *flat_pos = flat_data;
    for (tlv_t *item = value->head; item; item = item->next) {
        if (!item->size) {
            flat_pos[0] = item->type;
            flat_pos[1] = 0;
            flat_pos += 2;
            continue;
        }

        byte *item_data = item->value;
        size_t item_left = item->size;
        while (item_left) {
            size_t piece_size = (item_left > 255) ? 255 : item_left;
            flat_pos[0] = item->type;
            flat_pos[1] = piece_size;
            memcpy(&flat_pos[2], item_data, piece_size);
            item_left -= piece_size;
            flat_pos += piece_size + 2;
            item_data += piece_size;
        }
    }

    size_t size_left = flat_size;
    byte *piece_data = flat_data;
    if (!size_left) {
        tlv->buffer[tlv->pos++] = type;
        if (tlv->pos >= tlv->size) {
            if (tlv->pos) {
                if (tlv->on_flush)
                    tlv->on_flush(tlv->buffer, tlv->pos, tlv->context);
                tlv->pos = 0;
            }
        }
        tlv->buffer[tlv->pos++] = 0;
        if (tlv->pos >= tlv->size) {
            if (tlv->pos) {
                if (tlv->on_flush)
                    tlv->on_flush(tlv->buffer, tlv->pos, tlv->context);
                tlv->pos = 0;
            }
        }
        free(flat_data);
        return 0;
    }

    while (size_left) {
        uint8_t piece_size = (size_left > 255) ? 255 : size_left;

        tlv->buffer[tlv->pos++] = type;
        if (tlv->pos >= tlv->size) {
            if (tlv->pos) {
                if (tlv->on_flush)
                    tlv->on_flush(tlv->buffer, tlv->pos, tlv->context);
                tlv->pos = 0;
            }
        }

        tlv->buffer[tlv->pos++] = piece_size;
        if (tlv->pos >= tlv->size) {
            if (tlv->pos) {
                if (tlv->on_flush)
                    tlv->on_flush(tlv->buffer, tlv->pos, tlv->context);
                tlv->pos = 0;
            }
        }

        uint8_t pending = piece_size;
        while (pending) {
            size_t fit = pending;
            if (fit > tlv->size - tlv->pos)
                fit = tlv->size - tlv->pos;

            memcpy(&tlv->buffer[tlv->pos], piece_data, fit);
            pending -= fit;
            piece_data += fit;
            tlv->pos += fit;

            if (tlv->pos >= tlv->size) {
                if (tlv->pos) {
                    if (tlv->on_flush)
                        tlv->on_flush(tlv->buffer, tlv->pos, tlv->context);
                    tlv->pos = 0;
                }
            }
        }

        size_left -= piece_size;
    }

    free(flat_data);

    return 0;
}
