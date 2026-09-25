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
    const char *devices;
    const char *revision;
};

static const struct appfw_image appfw_image_record =
{
    .name = "fetch-cube",
    .devices = "STM32F407VG/STM32L053C8",
    .revision = "1"
};

volatile unsigned int appfw_boot_stage;

/* Copies one banner part into the running buffer, returns the new position. */
static unsigned int appfw_copy_part(char *banner, unsigned int position, const char *part)
{
    unsigned int index = 0u;

    while (part[index] != '\0')
    {
        banner[position++] = part[index++];
    }

    return position;
}

const char *appfw_signature(void)
{
    static char appfw_banner[64];
    unsigned int position = 0u;

    position = appfw_copy_part(appfw_banner, position, appfw_image_record.name);
    position = appfw_copy_part(appfw_banner, position, " @ ");
    position = appfw_copy_part(appfw_banner, position, appfw_image_record.devices);
    position = appfw_copy_part(appfw_banner, position, " rev");
    position = appfw_copy_part(appfw_banner, position, appfw_image_record.revision);
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
