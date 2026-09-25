
#ifndef __MEMCACHE_H__
#define __MEMCACHE_H__

#include <time.h>

#include "protocol.h"


struct fourtuple;

struct historical_result
{
    unsigned int succ;
    unsigned int fail1;
    unsigned int fail2;
};

void set_sid(struct fourtuple *f, int sid);
int get_sid(struct fourtuple *f);

void set_ttl(unsigned int daddr, unsigned char ttl);
void set_ttl_if_lt(unsigned int daddr, unsigned char ttl);
unsigned char get_ttl(unsigned int daddr);
void incr_ttl(unsigned int daddr);
void decr_ttl(unsigned int daddr);

void incr_succ(unsigned int daddr, int sid);
void incr_fail1(unsigned int daddr, int sid);
void incr_fail2(unsigned int daddr, int sid);
struct historical_result *get_hist_res(unsigned int daddr);

int load_ttl_from_redis();
int save_ttl_to_redis();

void save_historical_result_to_redis();
void load_historical_result_from_redis();


/*
 * Per-connection activity ledger.
 *
 * While the per-connection cache above only remembers which strategy was
 * dispatched, the ledger records what has actually been observed for a
 * connection during its current treatment cycle: the strategy that was
 * dispatched, how often a strategy has been tried, which activity and
 * censor-attack evidence was seen, which discrepancies were applied, and the
 * TTL hint used by insertion packets. It is the connection-facing half of the
 * cache TODO: over time we want to know, per connection and per host, which
 * discrepancies are applicable and which treatments actually work.
 *
 * The ledger is keyed by the connection 4-tuple. The record and its rules
 * belong to this module (the in-memory cache); other modules should consult
 * it through the accessors below instead of reaching into the record.
 */

/* Event evidence bits for conn_ledger.events */
#define LEDGER_EV_SEEN      (1 << 0)    /* flow observed */
#define LEDGER_EV_REQ       (1 << 1)    /* outgoing request observed */
#define LEDGER_EV_RESP      (1 << 2)    /* server response observed */
#define LEDGER_EV_TYPE1RST  (1 << 3)    /* type-1 reset attack evidence */
#define LEDGER_EV_TYPE2RST  (1 << 4)    /* type-2 reset attack evidence */
#define LEDGER_EV_VERIFIED  (1 << 5)    /* cycle closed by an evaluation */
#define LEDGER_EV_INJECTED  (1 << 6)    /* insertion packets were sent */

struct conn_ledger
{
    struct fourtuple fourtp;                    /* connection identity, the ledger's key */
    int sid;                                    /* strategy dispatched for this cycle */
    unsigned char attempts;                      /* count of dispatched strategies */
    unsigned char verdict;                       /* 0 unknown, 1 succeeded, 2 no response, 3 reset attack */
    unsigned short events;                       /* LEDGER_EV_* evidence */
    unsigned char discreps;                      /* INS_DISC_* discrepancies applied to this flow */
    unsigned int last_mark;                      /* last sequence number / txn id / evidence value */
    unsigned char ttl_hint;                      /* server-side TTL estimate for insertion packets */
    time_t ts;
    struct conn_ledger *prev;
    struct conn_ledger *next;
};

struct conn_ledger *ledger_ref(struct fourtuple *f);
struct conn_ledger *ledger_open(struct fourtuple *f);

int ledger_sid(struct conn_ledger *ledger);
unsigned short ledger_events(struct conn_ledger *ledger);
unsigned char ledger_ttl_hint(struct conn_ledger *ledger);

// debug
void conn_info_cache_summary();
void conn_info_cache_dump();
void host_info_cache_summary();
void host_info_cache_dump();
void conn_ledger_cache_summary();

#endif

