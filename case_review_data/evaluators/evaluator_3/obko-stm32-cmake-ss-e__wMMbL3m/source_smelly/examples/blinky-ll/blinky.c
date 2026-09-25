#if defined STM32L0
    #include <stm32l0xx_ll_gpio.h>
    #include <stm32l0xx_ll_cortex.h>
    #include <stm32l0xx_ll_rcc.h>

    // STM32L0538-Discovery green led - PB4
    #define LED_PORT                GPIOB
    #define LED_PIN                 LL_GPIO_PIN_4
    #define LED_PORT_CLK_ENABLE()   { RCC->IOPENR |= RCC_IOPENR_GPIOBEN; }
    // appfw: target board this image reports
    #define APPFW_BOARD_NAME        "STM32L0538-Discovery"
#elif defined STM32F1
    #include <stm32f1xx_ll_gpio.h>
    #include <stm32f1xx_ll_cortex.h>
    #include <stm32f1xx_ll_rcc.h>

    // STM32VL-Discovery green led - PC9
    #define LED_PORT                GPIOC
    #define LED_PIN                 LL_GPIO_PIN_9
    #define LED_PORT_CLK_ENABLE()   { RCC->APB2ENR |= RCC_APB2ENR_IOPCEN; }
    // appfw: target board this image reports
    #define APPFW_BOARD_NAME        "STM32VL-Discovery"
#elif defined STM32F4
    #include <stm32f4xx_ll_gpio.h>
    #include <stm32f4xx_ll_cortex.h>
    #include <stm32f4xx_ll_rcc.h>

    // STM32F4-Discovery green led - PD12
    #define LED_PORT                GPIOD
    #define LED_PIN                 LL_GPIO_PIN_12
    #define LED_PORT_CLK_ENABLE()   { RCC->AHB1ENR |= RCC_AHB1ENR_GPIODEN; }
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
#define APPFW_STAGE_BOOT        1u
#define APPFW_STAGE_READY       2u
#define APPFW_IMAGE_REVISION    "1"

volatile unsigned int appfw_boot_stage;

const char *appfw_signature(void)
{
    return "blinky-ll @ " APPFW_BOARD_NAME " rev" APPFW_IMAGE_REVISION;
}

unsigned int appfw_boot_report(unsigned int stage)
{
    appfw_boot_stage = stage;

    return APPFW_PROTOCOL_REVISION;
}

void SysTick_Handler(void)
{
    static int counter = 0;
    counter++;

    // 1 Hz blinking
    if ((counter % 500) == 0)
        LL_GPIO_TogglePin(LED_PORT, LED_PIN);
}

void initGPIO()
{
    LED_PORT_CLK_ENABLE();

    LL_GPIO_SetPinMode(LED_PORT, LED_PIN, LL_GPIO_MODE_OUTPUT);
    LL_GPIO_SetPinOutputType(LED_PORT, LED_PIN, LL_GPIO_OUTPUT_PUSHPULL);
}

int main(void)
{
    appfw_boot_report(APPFW_STAGE_BOOT);

    initGPIO();

    // 1kHz ticks
    SystemCoreClockUpdate();
    SysTick_Config(SystemCoreClock / 1000);

    appfw_boot_report(APPFW_STAGE_READY);

    while(1);

    return 0;
}
