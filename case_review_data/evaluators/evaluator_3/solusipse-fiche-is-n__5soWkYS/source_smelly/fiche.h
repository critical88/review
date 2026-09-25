/*
Fiche - Command line pastebin for sharing terminal output.

-------------------------------------------------------------------------------

License: MIT (http://www.opensource.org/licenses/mit-license.php)
Repository: https://github.com/solusipse/fiche/
Live example: http://termbin.com

-------------------------------------------------------------------------------

usage: fiche [-DepbsdolBuw].
             [-D] [-e] [-d domain] [-p port] [-s slug size]
             [-o output directory] [-B buffer size] [-u user name]
             [-l log file] [-b banlist] [-w whitelist]

Use netcat to push text - example:
$ cat fiche.c | nc localhost 9999

-------------------------------------------------------------------------------
*/

#ifndef FICHE_H
#define FICHE_H

#include <stdint.h>
#include <stdbool.h>


/**
 * @brief Used as a container for fiche settings. Create before
 *        the initialization
 *
 */
typedef struct Fiche_Settings {
    /**
     * @brief Domain used in output links
     */
    char *domain;

    /**
     * @brief Path to directory used for storing uploaded pastes
     */
    char *output_dir_path;

    /**
     * @brief Address on which fiche is waiting for connections
     */
    char *listen_addr;

    /**
     * @brief Port on which fiche is waiting for connections
     */
    uint16_t port;

    /**
     * @brief Length of a paste's name
     */
    uint8_t slug_len;

    /**
     * @brief If set, returns url with https prefix instead of http
     */
    bool https;

    /**
     * @brief Connection buffer length
     *
     * @remarks Length of this buffer limits max size of uploaded files
     */
    uint32_t buffer_len;

    /**
     * @brief Name of the user that runs fiche process
     */
    char *user_name;

    /**
     * @brief Path to the log file
     */
    char *log_file_path;

    /**
     * @brief Path to the file with banned IPs
     */
    char *banlist_path;

    /**
     * @brief Path to the file with whitelisted IPs
     */
    char *whitelist_path;



} Fiche_Settings;


/**
 * @brief Persistence seam: how uploaded pastes are named and stored
 */
typedef struct Fiche_Storage_Hooks {
    /**
     * @brief Picks a name for a new paste directory
     * @warning output has to be freed after using this hook!
     */
    void (*make_slug)(char **output, uint8_t length, uint8_t extra_length);

    /**
     * @brief Creates the directory that will hold a paste
     */
    int (*make_directory)(char *output_dir, char *slug);

    /**
     * @brief Writes received data into the paste directory
     */
    int (*store_paste)(const Fiche_Settings *settings, uint8_t *data,
            char *slug);
} Fiche_Storage_Hooks;


/**
 * @brief Audit seam: console reporting and per-connection log entries
 */
typedef struct Fiche_Audit_Hooks {
    /**
     * @brief Displays a status message
     */
    void (*report_status)(const char *format, ...);

    /**
     * @brief Displays an error message
     */
    void (*report_failure)(const char *format, ...);

    /**
     * @brief Displays a horizontal separator line
     */
    void (*write_line)(void);

    /**
     * @brief Saves a connection entry to the logfile
     */
    void (*note_connection)(const Fiche_Settings *settings, const char *ip,
            const char *hostname, const char *slug);
} Fiche_Audit_Hooks;


/**
 * @brief Identity seam: presentation-link setup and process ownership
 */
typedef struct Fiche_Identity_Hooks {
    /**
     * @brief Prefixes the domain with the http/https scheme
     * @warning hooks consumer has to free settings.domain afterwards!
     */
    int (*apply_domain)(Fiche_Settings *settings);

    /**
     * @brief Changes user running this program to the requested one
     */
    int (*adopt_user)(const Fiche_Settings *settings);
} Fiche_Identity_Hooks;


/**
 * @brief Admission seam: decides whether a connection may be served
 */
typedef struct Fiche_Admission_Hooks {
    /**
     * @brief Applies banlist and whitelist policy to an incoming connection
     */
    int (*admit_connection)(const Fiche_Settings *settings, const char *ip);
} Fiche_Admission_Hooks;


/**
 * @brief Uniform provider registration used by every fiche host
 *
 * @remarks Fiche is hosted in different environments: the command line
 *          daemon, long-running frontends and tests all drive the same
 *          engine. To keep one wiring path for all of them, every host
 *          registers a single complete provider table. The engine never
 *          falls back to built-in hooks on its own: a host that did not
 *          register a provider gets no behavior at all, so no host can
 *          silently end up with half a configuration.
 */
typedef struct Fiche_Provider {
    Fiche_Storage_Hooks storage;

    Fiche_Audit_Hooks audit;

    Fiche_Identity_Hooks identity;

    Fiche_Admission_Hooks admission;
} Fiche_Provider;


/**
 * @brief Service object carried through every engine seam
 *
 * @remarks Bundles the settings with the host-registered provider so
 *          the whole runtime state travels through the engine as one object.
 */
typedef struct Fiche_Service {
    Fiche_Settings settings;

    const Fiche_Provider *ops;
} Fiche_Service;


/**
 *  @brief Initializes Fiche_Settings instance
 */
void fiche_init(Fiche_Settings *settings);


/**
 *  @brief Runs fiche server
 *
 *  @return 0 if it was able to start, any other value otherwise
 */
int fiche_run(Fiche_Settings settings);


/**
 *  @brief Fills a provider table with the built-in hooks
 *
 *  @remarks Hosts call this first and then override the members they
 *           want to customize. Registering a complete table is part
 *           of the hosting contract.
 */
void fiche_provider_init(Fiche_Provider *ops);


/**
 *  @brief Boots the engine from a service object
 *
 *  @return 0 if it was able to start, any other value otherwise
 */
int fiche_service_boot(Fiche_Service *service);


/**
 * @brief array of symbols used in slug generation
 * @remarks defined in fiche.c
 */
extern const char *Fiche_Symbols;


#endif
