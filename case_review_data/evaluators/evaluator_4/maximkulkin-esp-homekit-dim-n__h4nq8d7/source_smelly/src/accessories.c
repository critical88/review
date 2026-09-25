#include <stdlib.h>
#include <string.h>
#include <homekit/types.h>

bool homekit_value_equal(homekit_value_t *a, homekit_value_t *b) {
    if (a->is_null != b->is_null)
        return false;

    if (a->format != b->format)
        return false;

    switch (a->format) {
        case homekit_format_bool:
            return a->bool_value == b->bool_value;
        case homekit_format_uint8:
        case homekit_format_uint16:
        case homekit_format_uint32:
        case homekit_format_uint64:
        case homekit_format_int:
            return a->int_value == b->int_value;
        case homekit_format_float:
            return a->float_value == b->float_value;
        case homekit_format_string:
            return !strcmp(a->string_value, b->string_value);
        case homekit_format_tlv: {
            if (!a->tlv_values && !b->tlv_values)
                return true;
            if (!a->tlv_values || !b->tlv_values)
                return false;

            // TODO: implement order independent comparison?
            tlv_t *ta = a->tlv_values->head, *tb = b->tlv_values->head;
            for (; ta && tb; ta=ta->next, tb=tb->next) {
                if (ta->type != tb->type || ta->size != tb->size)
                    return false;
                if (strncmp((char *)ta->value, (char *)tb->value, ta->size))
                    return false;
            }

            return (!ta && !tb);
        }
        case homekit_format_data:
            if (!a->data_value && !b->data_value)
                return true;

            if (!a->data_value || !b->data_value)
                return false;

            if (a->data_size != b->data_size)
                return false;

            return !memcmp(a->data_value, b->data_value, a->data_size);
    }

    return false;
}

void homekit_value_copy(homekit_value_t *dst, homekit_value_t *src) {
    memset(dst, 0, sizeof(*dst));

    dst->format = src->format;
    dst->is_null = src->is_null;
    dst->is_static = src->is_static;

    if (src->is_null)
        return;

    switch (src->format) {
        case homekit_format_bool:
            dst->bool_value = src->bool_value;
            break;
        case homekit_format_uint8:
            dst->uint8_value = src->uint8_value;
            break;
        case homekit_format_uint16:
            dst->uint16_value = src->uint16_value;
            break;
        case homekit_format_uint32:
            dst->uint32_value = src->uint32_value;
            break;
        case homekit_format_uint64:
            dst->uint64_value = src->uint64_value;
            break;
        case homekit_format_int:
            dst->int_value = src->int_value;
            break;
        case homekit_format_float:
            dst->float_value = src->float_value;
            break;
        case homekit_format_string:
            if (src->is_static) {
                dst->string_value = src->string_value;
            } else {
                dst->string_value = strdup(src->string_value);
            }
            break;
        case homekit_format_tlv: {
            if (src->is_static) {
                dst->tlv_values = src->tlv_values;
            } else {
                dst->tlv_values = tlv_new();
                for (tlv_t *v=src->tlv_values->head; v; v=v->next) {
                  tlv_add_value(dst->tlv_values, v->type, v->value, v->size);
                }
            }
            break;
        }
        case homekit_format_data:
            if (src->is_static) {
                dst->data_value = src->data_value;
                dst->data_size = src->data_size;
            } else {
                dst->data_size = src->data_size;
                dst->data_value = malloc(src->data_size);
                memcpy(dst->data_value, src->data_value, src->data_size);
            }
            break;
        default:
            // unknown format
            break;
    }
}


homekit_value_t *homekit_value_clone(homekit_value_t *value) {
    homekit_value_t *copy = malloc(sizeof(homekit_value_t));

    memset(copy, 0, sizeof(*copy));

    copy->format = value->format;
    copy->is_null = value->is_null;
    copy->is_static = value->is_static;

    if (!value->is_null) {
        switch (value->format) {
            case homekit_format_bool:
                copy->bool_value = value->bool_value;
                break;
            case homekit_format_uint8:
                copy->uint8_value = value->uint8_value;
                break;
            case homekit_format_uint16:
                copy->uint16_value = value->uint16_value;
                break;
            case homekit_format_uint32:
                copy->uint32_value = value->uint32_value;
                break;
            case homekit_format_uint64:
                copy->uint64_value = value->uint64_value;
                break;
            case homekit_format_int:
                copy->int_value = value->int_value;
                break;
            case homekit_format_float:
                copy->float_value = value->float_value;
                break;
            case homekit_format_string:
                if (value->is_static) {
                    copy->string_value = value->string_value;
                } else {
                    copy->string_value = strdup(value->string_value);
                }
                break;
            case homekit_format_tlv:
                if (value->is_static) {
                    copy->tlv_values = value->tlv_values;
                } else {
                    copy->tlv_values = malloc(sizeof(tlv_values_t));
                    copy->tlv_values->head = NULL;

                    for (tlv_t *item = value->tlv_values->head; item; item = item->next) {
                        size_t item_size = item->size;
                        byte *item_data = NULL;

                        if (item_size) {
                            item_data = malloc(item_size);
                            if (!item_data)
                                continue;
                            memcpy(item_data, item->value, item_size);
                        }

                        tlv_t *item_tlv = malloc(sizeof(tlv_t));
                        if (!item_tlv)
                            continue;

                        item_tlv->type = item->type;
                        item_tlv->size = item_size;
                        item_tlv->value = item_data;
                        item_tlv->next = NULL;

                        if (!copy->tlv_values->head) {
                            copy->tlv_values->head = item_tlv;
                        } else {
                            tlv_t *tail = copy->tlv_values->head;
                            while (tail->next)
                                tail = tail->next;
                            tail->next = item_tlv;
                        }
                    }
                }
                break;
            case homekit_format_data:
                if (value->is_static) {
                    copy->data_value = value->data_value;
                    copy->data_size = value->data_size;
                } else {
                    copy->data_size = value->data_size;
                    copy->data_value = malloc(value->data_size);
                    memcpy(copy->data_value, value->data_value, value->data_size);
                }
                break;
            default:
                // unknown format
                break;
        }
    }

    return copy;
}

void homekit_value_destruct(homekit_value_t *value) {
    if (value->is_null)
        return;

    switch (value->format) {
        case homekit_format_string:
            if (!value->is_static && value->string_value)
                free(value->string_value);
            break;
        case homekit_format_tlv:
            if (!value->is_static && value->tlv_values)
                tlv_free(value->tlv_values);
            break;
        case homekit_format_data:
            if (!value->is_static && value->data_value)
                free(value->data_value);
            break;
        default:
            // unknown format
            break;
    }

    value->is_null = true;
}

void homekit_value_free(homekit_value_t *value) {
    homekit_value_destruct(value);
    free(value);
}


static size_t align_size(size_t size) {
    if (size % sizeof(void*)) {
        size += sizeof(void*) - size % sizeof(void*);
    }
    return size;
}


homekit_characteristic_t* homekit_characteristic_clone(homekit_characteristic_t* ch) {
    size_t type_len = strlen(ch->type) + 1;
    size_t description_len = ch->description ? strlen(ch->description) + 1 : 0;

    size_t size = sizeof(homekit_characteristic_t) + type_len + description_len;
    if (size % sizeof(void*))
        size += sizeof(void*) - size % sizeof(void*);

    if (ch->min_value)
        size += sizeof(float);
    if (ch->max_value)
        size += sizeof(float);
    if (ch->min_step)
        size += sizeof(float);
    if (ch->max_len)
        size += sizeof(int);
    if (ch->max_data_len)
        size += sizeof(int);
    if (ch->valid_values.count) {
        size_t valid_values_bytes = sizeof(uint8_t) * ch->valid_values.count;
        if (valid_values_bytes % sizeof(void*))
            valid_values_bytes += sizeof(void*) - valid_values_bytes % sizeof(void*);
        size += valid_values_bytes;
    }
    if (ch->valid_values_ranges.count) {
        size_t ranges_bytes = sizeof(homekit_valid_values_range_t) * ch->valid_values_ranges.count;
        if (ranges_bytes % sizeof(void*))
            ranges_bytes += sizeof(void*) - ranges_bytes % sizeof(void*);
        size += ranges_bytes;
    }
    if (ch->callback) {
        homekit_characteristic_change_callback_t *c = ch->callback;
        while (c) {
            size_t callback_bytes = sizeof(homekit_characteristic_change_callback_t);
            if (callback_bytes % sizeof(void*))
                callback_bytes += sizeof(void*) - callback_bytes % sizeof(void*);
            size += callback_bytes;
            c = c->next;
        }
    }

    uint8_t* p = calloc(1, size);

    homekit_characteristic_t* clone = (homekit_characteristic_t*) p;
    p += sizeof(homekit_characteristic_t);

    clone->service = ch->service;
    clone->id = ch->id;
    clone->type = (char*) p;
    strncpy((char*) p, ch->type, type_len);
    p[type_len - 1] = 0;
    p += type_len;

    clone->description = (char*) p;
    strncpy((char*) p, ch->description, description_len);
    p[description_len - 1] = 0;
    p += description_len;

    if ((size_t)p % sizeof(void*))
        p += sizeof(void*) - (size_t)p % sizeof(void*);

    clone->format = ch->format;
    clone->unit = ch->unit;
    clone->permissions = ch->permissions;

    // take over the current characteristic value, duplicating every
    // dynamically owned payload along the way
    memset(&clone->value, 0, sizeof(clone->value));
    clone->value.format = ch->value.format;
    clone->value.is_null = ch->value.is_null;
    clone->value.is_static = ch->value.is_static;

    if (!ch->value.is_null) {
        switch (ch->value.format) {
            case homekit_format_bool:
                clone->value.bool_value = ch->value.bool_value;
                break;
            case homekit_format_uint8:
                clone->value.uint8_value = ch->value.uint8_value;
                break;
            case homekit_format_uint16:
                clone->value.uint16_value = ch->value.uint16_value;
                break;
            case homekit_format_uint32:
                clone->value.uint32_value = ch->value.uint32_value;
                break;
            case homekit_format_uint64:
                clone->value.uint64_value = ch->value.uint64_value;
                break;
            case homekit_format_int:
                clone->value.int_value = ch->value.int_value;
                break;
            case homekit_format_float:
                clone->value.float_value = ch->value.float_value;
                break;
            case homekit_format_string:
                clone->value.string_value = ch->value.is_static
                    ? ch->value.string_value
                    : strdup(ch->value.string_value);
                break;
            case homekit_format_tlv:
                if (ch->value.is_static) {
                    clone->value.tlv_values = ch->value.tlv_values;
                } else {
                    clone->value.tlv_values = malloc(sizeof(tlv_values_t));
                    clone->value.tlv_values->head = NULL;

                    for (tlv_t *item = ch->value.tlv_values->head; item; item = item->next) {
                        size_t item_size = item->size;
                        byte *item_data = NULL;

                        if (item_size) {
                            item_data = malloc(item_size);
                            if (!item_data)
                                continue;
                            memcpy(item_data, item->value, item_size);
                        }

                        tlv_t *item_tlv = malloc(sizeof(tlv_t));
                        if (!item_tlv)
                            continue;

                        item_tlv->type = item->type;
                        item_tlv->size = item_size;
                        item_tlv->value = item_data;
                        item_tlv->next = NULL;

                        if (!clone->value.tlv_values->head) {
                            clone->value.tlv_values->head = item_tlv;
                        } else {
                            tlv_t *tail = clone->value.tlv_values->head;
                            while (tail->next)
                                tail = tail->next;
                            tail->next = item_tlv;
                        }
                    }
                }
                break;
            case homekit_format_data:
                if (ch->value.is_static) {
                    clone->value.data_value = ch->value.data_value;
                    clone->value.data_size = ch->value.data_size;
                } else {
                    clone->value.data_size = ch->value.data_size;
                    clone->value.data_value = malloc(ch->value.data_size);
                    memcpy(clone->value.data_value, ch->value.data_value, ch->value.data_size);
                }
                break;
            default:
                // unknown format
                break;
        }
    }

    if (ch->min_value) {
        clone->min_value = (float*) p;
        *clone->min_value = *ch->min_value;
        p += sizeof(float);
    }

    if (ch->max_value) {
        clone->max_value = (float*) p;
        *clone->max_value = *ch->max_value;
        p += sizeof(float);
    }

    if (ch->min_step) {
        clone->min_step = (float*) p;
        *clone->min_step = *ch->min_step;
        p += sizeof(float);
    }

    if (ch->max_len) {
        clone->max_len = (int*) p;
        *clone->max_len = *ch->max_len;
        p += sizeof(int);
    }

    if (ch->max_data_len) {
        clone->max_data_len = (int*) p;
        *clone->max_data_len = *ch->max_data_len;
        p += sizeof(int);
    }

    if (ch->valid_values.count) {
        clone->valid_values.count = ch->valid_values.count;
        clone->valid_values.values = (uint8_t*) p;
        memcpy(clone->valid_values.values, ch->valid_values.values,
               sizeof(uint8_t) * ch->valid_values.count);

        size_t valid_values_bytes = sizeof(uint8_t) * ch->valid_values.count;
        if (valid_values_bytes % sizeof(void*))
            valid_values_bytes += sizeof(void*) - valid_values_bytes % sizeof(void*);
        p += valid_values_bytes;
    }

    if (ch->valid_values_ranges.count) {
        int c = ch->valid_values_ranges.count;
        clone->valid_values_ranges.count = c;
        clone->valid_values_ranges.ranges = (homekit_valid_values_range_t*) p;
        memcpy(clone->valid_values_ranges.ranges,
               ch->valid_values_ranges.ranges,
               sizeof(homekit_valid_values_range_t*) * c);

        size_t ranges_bytes = sizeof(homekit_valid_values_range_t*) * c;
        if (ranges_bytes % sizeof(void*))
            ranges_bytes += sizeof(void*) - ranges_bytes % sizeof(void*);
        p += ranges_bytes;
    }

    if (ch->callback) {
        int c = 1;
        homekit_characteristic_change_callback_t *callback_in = ch->callback;
        clone->callback = (homekit_characteristic_change_callback_t*)p;

        homekit_characteristic_change_callback_t *callback_out = clone->callback;
        memcpy(callback_out, callback_in, sizeof(*callback_out));

        while (callback_in->next) {
            callback_in = callback_in->next;
            callback_out->next = callback_out + 1;
            callback_out = callback_out->next;
            memcpy(callback_out, callback_in, sizeof(*callback_out));
            c++;
        }
        callback_out->next = NULL;

        size_t callback_bytes = sizeof(homekit_characteristic_change_callback_t);
        if (callback_bytes % sizeof(void*))
            callback_bytes += sizeof(void*) - callback_bytes % sizeof(void*);
        p += callback_bytes * c;
    }

    clone->getter = ch->getter;
    clone->setter = ch->setter;
    clone->getter_ex = ch->getter_ex;
    clone->setter_ex = ch->setter_ex;
    clone->context = ch->context;

    return clone;
}

homekit_service_t* homekit_service_clone(homekit_service_t* service) {
    size_t type_len = strlen(service->type) + 1;
    size_t size = align_size(sizeof(homekit_service_t) + type_len);

    if (service->linked) {
        int i = 0;
        while (service->linked[i])
            i++;

        size += sizeof(homekit_service_t*) * (i + 1);
    }
    if (service->characteristics) {
        int i = 0;
        while (service->characteristics[i])
            i++;

        size += sizeof(homekit_characteristic_t*) * (i + 1);
    }

    uint8_t *p = calloc(1, size);
    homekit_service_t *clone = (homekit_service_t*) p;
    p += sizeof(homekit_service_t);
    clone->accessory = service->accessory;
    clone->id = service->id;
    clone->type = strncpy((char*) p, service->type, type_len);
    p[type_len - 1] = 0;
    p += align_size(type_len);

    clone->hidden = service->hidden;
    clone->primary = service->primary;

    if (service->linked) {
        clone->linked = (homekit_service_t**) p;
        int i = 0;
        while (service->linked[i]) {
            clone->linked[i] = service->linked[i];
            i++;
        }
        clone->linked[i] = NULL;
        p += (i + 1) * sizeof(homekit_service_t*);
    }

    if (service->characteristics) {
        clone->characteristics = (homekit_characteristic_t**) p;
        int i = 0;
        while (service->characteristics[i]) {
            clone->characteristics[i] = service->characteristics[i];
            i++;
        }
        clone->characteristics[i] = NULL;
        p += (i + 1) * sizeof(homekit_characteristic_t*);
    }

    return clone;
}

homekit_accessory_t* homekit_accessory_clone(homekit_accessory_t* ac) {
    size_t size = sizeof(homekit_accessory_t);
    if (ac->services) {
        for (int i=0; ac->services[i]; i++) {
            size += sizeof(homekit_service_t*);
        }
        size += sizeof(homekit_service_t*);
    }

    uint8_t* p = calloc(1, size);
    homekit_accessory_t* clone = (homekit_accessory_t*) p;
    p += align_size(sizeof(homekit_accessory_t));

    clone->id = ac->id;
    clone->category = ac->category;
    clone->config_number = ac->config_number;

    if (ac->services) {
        clone->services = (homekit_service_t**) p;
        int i = 0;
        while (ac->services[i]) {
            clone->services[i] = ac->services[i];
            i++;
        }
        clone->services[i] = NULL;

        p += sizeof(homekit_service_t*) * (i + 1);
    }

    return clone;
}


homekit_value_t homekit_characteristic_default_getter_ex(const homekit_characteristic_t *ch) {
    if (ch->getter) {
        return ch->getter();
    }

    homekit_value_t value = ch->value;
    value.is_static = true;
    return value;
}


void homekit_characteristic_default_setter_ex(homekit_characteristic_t *ch, homekit_value_t value) {
    if (ch->setter) {
        ch->setter(value);
    } else {
        homekit_value_destruct(&ch->value);
        ch->value = value;
    }
}


void homekit_characteristic_add_notify_callback(
    homekit_characteristic_t *ch,
    homekit_characteristic_change_callback_fn function,
    void *context
) {
    homekit_characteristic_change_callback_t *new_callback = malloc(sizeof(homekit_characteristic_change_callback_t));
    new_callback->function = function;
    new_callback->context = context;
    new_callback->next = NULL;

    if (!ch->callback) {
        ch->callback = new_callback;
    } else {
        homekit_characteristic_change_callback_t *callback = ch->callback;
        if (callback->function == function && callback->context == context) {
            free(new_callback);
            return;
        }

        while (callback->next) {
            if (callback->next->function == function && callback->next->context == context) {
                free(new_callback);
                return;
            }
            callback = callback->next;
        }

        callback->next = new_callback;
    }
}


void homekit_characteristic_remove_notify_callback(
    homekit_characteristic_t *ch,
    homekit_characteristic_change_callback_fn function,
    void *context
) {
    while (ch->callback) {
        if (ch->callback->function != function || ch->callback->context != context) {
            break;
        }

        homekit_characteristic_change_callback_t *c = ch->callback;
        ch->callback = ch->callback->next;
        free(c);
    }

    if (!ch->callback)
        return;

    homekit_characteristic_change_callback_t *callback = ch->callback;
    while (callback->next) {
        if (callback->next->function == function && callback->next->context == context) {
            homekit_characteristic_change_callback_t *c = callback->next;
            callback->next = callback->next->next;
            free(c);
        } else {
            callback = callback->next;
        }
    }
}


// Removes particular callback from all characteristics
void homekit_accessories_clear_notify_callbacks(
    homekit_accessory_t **accessories,
    homekit_characteristic_change_callback_fn function,
    void *context
) {
    for (homekit_accessory_t **accessory_it = accessories; *accessory_it; accessory_it++) {
        homekit_accessory_t *accessory = *accessory_it;

        for (homekit_service_t **service_it = accessory->services; *service_it; service_it++) {
            homekit_service_t *service = *service_it;

            for (homekit_characteristic_t **ch_it = service->characteristics; *ch_it; ch_it++) {
                homekit_characteristic_t *ch = *ch_it;

                homekit_characteristic_remove_notify_callback(ch, function, context);
            }
        }
    }
}


bool homekit_characteristic_has_notify_callback(
    const homekit_characteristic_t *ch,
    homekit_characteristic_change_callback_fn function,
    void *context
) {
    homekit_characteristic_change_callback_t *callback = ch->callback;
    while (callback) {
        if (callback->function == function && callback->context == context)
            return true;

        callback = callback->next;
    }

    return false;
}

