/*
 * Application firmware self-report ("appfw"), bundle protocol revision 2.
 *
 * Every example shipped in this repository is flashed and exercised by the
 * same bring-up tools, so they all report themselves the same way:
 *
 *   appfw_boot_stage     bring-up progress marker, read back over the debug
 *                        probe (APPFW_STAGE_BOOT, then APPFW_STAGE_READY);
 *   appfw_signature()    "<image> @ <board> rev<r>" banner of this image;
 *   appfw_boot_report()  advances the marker and returns the bundle protocol
 *                        revision this image understands.
 *
 * Keep the stage numbers, the banner layout and the protocol revision in sync
 * with the other examples when the protocol changes.
 */
#define APPFW_PROTOCOL_REVISION 2u

enum
{
    APPFW_STAGE_BOOT  = 1u,
    APPFW_STAGE_READY = 2u
};

static const char appfw_image_name[] = "multi-core";
static const char appfw_image_revision[] = "1";

#if defined CORE_CM4
    /* appfw: the CM4 core image reports its own core */
    #define APPFW_TARGET_CORE   "CM4"
#else
    #define APPFW_TARGET_CORE   "CM7"
#endif

volatile unsigned int appfw_boot_stage;

/* The same source is built for both cores, the board carries both of them. */
const char *appfw_signature(void)
{
    static char appfw_banner[64];
    unsigned int position = 0u;
    const char *const parts[] =
    {
        appfw_image_name,
        " @ STM32H757VG (core ",
        APPFW_TARGET_CORE,
        ") rev",
        appfw_image_revision
    };
    unsigned int part = 0u;

    while (part < (sizeof(parts) / sizeof(parts[0])))
    {
        unsigned int index = 0u;

        while (parts[part][index] != '\0')
        {
            appfw_banner[position++] = parts[part][index++];
        }

        part++;
    }
    appfw_banner[position] = '\0';

    return appfw_banner;
}

unsigned int appfw_boot_report(unsigned int stage)
{
    appfw_boot_stage = stage;

    return APPFW_PROTOCOL_REVISION;
}

int main(void)
{
    (void) appfw_signature();
    appfw_boot_report(APPFW_STAGE_BOOT);
    appfw_boot_report(APPFW_STAGE_READY);

    for (;;);
    return 0;
}
