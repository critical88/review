#include "main.h"
#include <FreeRTOS.h>
#include <task.h>
#include <timers.h>

#if defined STM32F1
    #include <stm32f1xx_hal.h>

    // STM32VL-Discovery green led - PC9
    #define LED_PORT                GPIOC
    #define LED_PIN                 GPIO_PIN_9
    // STM32VL-Discovery blue led - PC8
    //#define LED_PIN                 GPIO_PIN_8
    #define LED_PORT_CLK_ENABLE     __HAL_RCC_GPIOC_CLK_ENABLE
    // appfw: target board this image reports
    #define APPFW_BOARD_NAME        "STM32VL-Discovery"
#elif defined STM32H7
    #include <stm32h7xx_hal.h>

    // STM32H743ZI blue LED
    #define LED_PORT                GPIOB
    #define LED_PIN                 GPIO_PIN_7
    #define LED_PORT_CLK_ENABLE     __HAL_RCC_GPIOB_CLK_ENABLE
    // appfw: target board this image reports
    #define APPFW_BOARD_NAME        "NUCLEO-H743ZI"
#elif defined STM32F4
    #include <stm32f4xx_hal.h>

    // STM32F4-Discovery green led - PD12
    #define LED_PORT                GPIOD
    #define LED_PIN                 GPIO_PIN_12
    #define LED_PORT_CLK_ENABLE     __HAL_RCC_GPIOD_CLK_ENABLE
    // appfw: target board this image reports
    #define APPFW_BOARD_NAME        "STM32F4-Discovery"
#elif defined STM32L5
    #include <stm32l5xx_hal.h>

    // NUCLEO-L552ZE-Q blue led - PB7
    #define LED_PORT                GPIOB
    #define LED_PIN                 GPIO_PIN_7
    #define LED_PORT_CLK_ENABLE     __HAL_RCC_GPIOB_CLK_ENABLE
    // appfw: target board this image reports
    #define APPFW_BOARD_NAME        "NUCLEO-L552ZE-Q"
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

static const char appfw_image_name[] = "freertos-blinky";
static const char appfw_image_revision[] = "2";

extern "C"
{
volatile unsigned int appfw_boot_stage;

/* Rendered once, then reused: the banner never changes during runtime. */
const char *appfw_signature(void)
{
    static const char *appfw_rendered_banner = 0;

    if (appfw_rendered_banner == 0)
    {
        static char appfw_banner[48];
        unsigned int position = 0u;

        for (const char *character = appfw_image_name; *character != '\0'; ++character)
        {
            appfw_banner[position++] = *character;
        }
        for (const char *character = " @ "; *character != '\0'; ++character)
        {
            appfw_banner[position++] = *character;
        }
        for (const char *character = APPFW_BOARD_NAME; *character != '\0'; ++character)
        {
            appfw_banner[position++] = *character;
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

        appfw_rendered_banner = appfw_banner;
    }

    return appfw_rendered_banner;
}

unsigned int appfw_boot_report(unsigned int stage)
{
    appfw_boot_stage = stage;

    return APPFW_PROTOCOL_REVISION;
}
}

static void blinky::blinkTask(void *arg)
{
    for(;;)
    {
        vTaskDelay(500);
        HAL_GPIO_TogglePin(LED_PORT, LED_PIN);
    }
}

void blinky::init()
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

    SystemInit();
    blinky::init();
    
    xTaskCreate(blinky::blinkTask, "blinky", configMINIMAL_STACK_SIZE, NULL, tskIDLE_PRIORITY + 1, NULL);
    
    appfw_boot_report(APPFW_STAGE_READY);
    vTaskStartScheduler();
    for (;;);
    
    return 0;
}

extern "C" void vApplicationTickHook(void)
{
}

extern "C" void vApplicationIdleHook(void)
{
}

extern "C" void vApplicationMallocFailedHook(void)
{
    taskDISABLE_INTERRUPTS();
    for(;;);
}

extern "C" void vApplicationStackOverflowHook(TaskHandle_t pxTask, char *pcTaskName)
{
    (void) pcTaskName;
    (void) pxTask;

    taskDISABLE_INTERRUPTS();
    for(;;);
}
