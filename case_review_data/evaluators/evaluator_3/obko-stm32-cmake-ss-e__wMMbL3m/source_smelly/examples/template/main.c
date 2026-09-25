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
#define APPFW_STAGE_BOOT        1u
#define APPFW_STAGE_READY       2u

struct appfw_image
{
    const char *name;
    const char *board;
    const char *revision;
};

static const struct appfw_image appfw_firmware_image =
{
    "template",
    "STM32F4-Discovery",
    "1"
};

volatile unsigned int appfw_boot_stage;

/* "<image> @ <board> rev<r>", in one pass over the record fields. */
const char *appfw_signature(void)
{
    static char appfw_banner[48];
    const char *const pieces[] =
    {
        appfw_firmware_image.name,
        " @ ",
        appfw_firmware_image.board,
        " rev",
        appfw_firmware_image.revision
    };
    unsigned int piece = 0u;
    unsigned int position = 0u;

    while (piece < (sizeof(pieces) / sizeof(pieces[0])))
    {
        unsigned int index = 0u;

        while (pieces[piece][index] != '\0')
        {
            appfw_banner[position++] = pieces[piece][index++];
        }

        piece++;
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
    appfw_boot_report(APPFW_STAGE_BOOT);
    appfw_boot_report(APPFW_STAGE_READY);

    for (;;);
    return 0;
}
