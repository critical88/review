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
#define APPFW_PROTOCOL_REVISION      2u
#define APPFW_STAGE_BOOT             1u
#define APPFW_STAGE_READY            2u
#define APPFW_IMAGE_REVISION         1
#define APPFW_STRINGIFY_TOKEN(x)     #x
#define APPFW_STRINGIFY(revision)    APPFW_STRINGIFY_TOKEN(revision)
#define APPFW_TARGET_BOARD           "STM32F4-Discovery"

volatile unsigned int appfw_boot_stage;

const char *appfw_signature(void)
{
    return "custom-linker-script @ " APPFW_TARGET_BOARD " rev" APPFW_STRINGIFY(APPFW_IMAGE_REVISION);
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
