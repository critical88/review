#if defined STM32L0
    #include <stm32l0xx_hal.h>

    // STM32L0538-Discovery green led - PB4
    #define LED_PORT                GPIOB
    #define LED_PIN                 GPIO_PIN_4
    #define LED_PORT_CLK_ENABLE     __HAL_RCC_GPIOB_CLK_ENABLE
    // appfw: target board this image reports
    #define APPFW_BOARD_NAME        "STM32L0538-Discovery"
#elif defined STM32F1
    #include <stm32f1xx_hal.h>

    // STM32VL-Discovery green led - PC9
    #define LED_PORT                GPIOC
    #define LED_PIN                 GPIO_PIN_9
    #define LED_PORT_CLK_ENABLE     __HAL_RCC_GPIOC_CLK_ENABLE
    // appfw: target board this image reports
    #define APPFW_BOARD_NAME        "STM32VL-Discovery"
#elif defined STM32F4
    #include <stm32f4xx_hal.h>

    // STM32F4-Discovery green led - PD12
    #define LED_PORT                GPIOD
    #define LED_PIN                 GPIO_PIN_12
    #define LED_PORT_CLK_ENABLE     __HAL_RCC_GPIOD_CLK_ENABLE
    // appfw: target board this image reports
    #define APPFW_BOARD_NAME        "STM32F4-Discovery"
#endif

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
    APPFW_STAGE_BOOT  = 1,
    APPFW_STAGE_READY = 2
};

struct appfw_field
{
    const char *literal;
};

/* Banner order is fixed by the protocol: image, board, revision. */
static const appfw_field appfw_banner_fields[] =
{
    { "blinky" },
    { " @ " APPFW_BOARD_NAME },
    { " rev1" }
};

extern "C"
{
volatile unsigned int appfw_boot_stage;

const char *appfw_signature(void)
{
    static char appfw_banner[48];
    unsigned int position = 0u;
    unsigned int field = 0u;
    const unsigned int fields = sizeof(appfw_banner_fields) / sizeof(appfw_banner_fields[0]);

    while (field < fields)
    {
        unsigned int char_index = 0u;

        while (appfw_banner_fields[field].literal[char_index] != '\0')
        {
            appfw_banner[position++] = appfw_banner_fields[field].literal[char_index++];
        }

        field++;
    }
    appfw_banner[position] = '\0';

    return appfw_banner;
}

unsigned int appfw_boot_report(unsigned int stage)
{
    appfw_boot_stage = stage;

    return APPFW_PROTOCOL_REVISION;
}
}

//This prevent name mangling for functions used in C/assembly files.
extern "C"
{
    void SysTick_Handler(void)
    {
        HAL_IncTick();

        // 1 Hz blinking
        if ((HAL_GetTick() % 500) == 0)
        {
            HAL_GPIO_TogglePin(LED_PORT, LED_PIN);
        }
    }
}

void initGPIO()
{
    GPIO_InitTypeDef GPIO_Config;

    GPIO_Config.Mode = GPIO_MODE_OUTPUT_PP;
    GPIO_Config.Pull = GPIO_NOPULL;
    GPIO_Config.Speed = GPIO_SPEED_FREQ_HIGH;

    GPIO_Config.Pin = LED_PIN;

    LED_PORT_CLK_ENABLE();
    HAL_GPIO_Init(LED_PORT, &GPIO_Config);
}

int main(void)
{
    appfw_boot_report(APPFW_STAGE_BOOT);

    HAL_Init();
    initGPIO();
    // 1kHz ticks
    HAL_SYSTICK_Config(SystemCoreClock / 1000);

    appfw_boot_report(APPFW_STAGE_READY);

    while(1);
    return 0;
}
