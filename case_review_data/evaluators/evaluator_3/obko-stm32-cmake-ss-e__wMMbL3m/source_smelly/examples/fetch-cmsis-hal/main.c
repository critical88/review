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

static const char appfw_image_name[] = "fetch-cmsis-hal";
static const char *const appfw_target_devices[] = { "STM32F407VG", "STM32L053C8" };
static const char appfw_image_revision[] = "1";

volatile unsigned int appfw_boot_stage;

/* One source, two images: the banner lists both boards of the example. */
const char *appfw_signature(void)
{
    static char appfw_banner[64];
    unsigned int device = 0u;
    unsigned int position = 0u;

    for (const char *character = appfw_image_name; *character != '\0'; ++character)
    {
        appfw_banner[position++] = *character;
    }
    while (device < (sizeof(appfw_target_devices) / sizeof(appfw_target_devices[0])))
    {
        for (const char *character = device == 0u ? " @ " : "/"; *character != '\0'; ++character)
        {
            appfw_banner[position++] = *character;
        }
        for (const char *character = appfw_target_devices[device]; *character != '\0'; ++character)
        {
            appfw_banner[position++] = *character;
        }

        device++;
    }
    for (const char *character = " rev"; *character != '\0'; ++character)
    {
        appfw_banner[position++] = *character;
    }
    for (const char *character = appfw_image_revision; *character != '\0'; ++character)
    {
        appfw_banner[position++] = *character;
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
    appfw_boot_report(APPFW_STAGE_READY);

    for (;;);
    return 0;
}
