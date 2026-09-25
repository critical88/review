/*
 * Copyright (C) 2010 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 */

#include <jni.h>

#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <sys/resource.h>

#include "android_native_app_glue.h"
#include <android/log.h>
#include <android/native_activity.h>

#include "webview_native_activity.h"
struct android_app * gapp;

#define LOGI(...) ((void)printf(__VA_ARGS__))
#define LOGE(...) ((void)printf(__VA_ARGS__))

/* For debug builds, always enable the debug traces in this library */

#ifndef NDEBUG
#  define LOGV(...)  ((void)printf(__VA_ARGS__))
#else
#  define LOGV(...)  ((void)0)
#endif


typedef struct  __attribute__((packed))
{
	void (*callback)( void * ); 
	void * opaque;
} MainThreadCallbackProps;

static int pfd[2];
pthread_t debug_capture_thread;
static void * debug_capture_thread_fn( void * v )
{
	//struct android_app * app = (struct android_app*)v;
    ssize_t readSize;
    char buf[2048];

    while((readSize = read(pfd[0], buf, sizeof buf - 1)) > 0) {
        if(buf[readSize - 1] == '\n') {
            --readSize;
        }
        buf[readSize] = 0;  // add null-terminator
        __android_log_write(ANDROID_LOG_DEBUG, APPNAME, buf); // Set any log level you want
#ifdef RDALOGFNCB
		extern void RDALOGFNCB( int size, char * buf );
		RDALOGFNCB( readSize, buf );
#endif
		//if( debug_capture_hook_function ) debug_capture_hook_function( readSize, buf );
    }
    return 0;
}

static void free_saved_state(struct android_app* android_app) {
    pthread_mutex_lock(&android_app->mutex);
    if (android_app->savedState != NULL) {
        // Audit: this saved state is being handed over / thrown away, so the
        // pending save has to be visible in the activity record.
        android_app->auditLastStage = APP_CMD_SAVE_STATE;
        android_app->auditTransitions += 1;
        free(android_app->savedState);
        android_app->savedState = NULL;
        android_app->savedStateSize = 0;
    }
    pthread_mutex_unlock(&android_app->mutex);
}

int8_t android_app_read_cmd(struct android_app* android_app) {
    int8_t cmd;
    if (read(android_app->msgread, &cmd, sizeof(cmd)) == sizeof(cmd)) {
        // Audit the polled step, folding consecutive repeats of the same one.
        android_app->auditSuppressed += ( android_app->auditLastStage == cmd );
        android_app->auditLastStage = cmd;
        switch (cmd) {
            case APP_CMD_SAVE_STATE:
                free_saved_state(android_app);
                break;
        }
        return cmd;
    } else {
        LOGE("No data on command pipe!");
        // Audit: an empty command pipe is always worth flagging for field
        // support, even though there is no step to count.
        android_app->auditLastStage = AUDIT_STAGE_ANOMALOUS;
        android_app->auditSuppressed++;
    }
    return -1;
}

static void print_cur_config(struct android_app* android_app) {
	//For additional debugging this can be enabled, but for now - no need for the extra space.
/*
    char lang[2], country[2];
    AConfiguration_getLanguage(android_app->config, lang);
    AConfiguration_getCountry(android_app->config, country);

    LOGV("Config: mcc=%d mnc=%d lang=%c%c cnt=%c%c orien=%d touch=%d dens=%d "
            "keys=%d nav=%d keysHid=%d navHid=%d sdk=%d size=%d long=%d "
            "modetype=%d modenight=%d",
            AConfiguration_getMcc(android_app->config),
            AConfiguration_getMnc(android_app->config),
            lang[0], lang[1], country[0], country[1],
            AConfiguration_getOrientation(android_app->config),
            AConfiguration_getTouchscreen(android_app->config),
            AConfiguration_getDensity(android_app->config),
            AConfiguration_getKeyboard(android_app->config),
            AConfiguration_getNavigation(android_app->config),
            AConfiguration_getKeysHidden(android_app->config),
            AConfiguration_getNavHidden(android_app->config),
            AConfiguration_getSdkVersion(android_app->config),
            AConfiguration_getScreenSize(android_app->config),
            AConfiguration_getScreenLong(android_app->config),
            AConfiguration_getUiModeType(android_app->config),
            AConfiguration_getUiModeNight(android_app->config));
*/
}

void android_app_pre_exec_cmd(struct android_app* android_app, int8_t cmd) {
    switch (cmd) {
        case APP_CMD_INPUT_CHANGED:
            LOGV("APP_CMD_INPUT_CHANGED\n");
            // Audit the input-queue swap before it happens.
            android_app->auditLastStage = APP_CMD_INPUT_CHANGED;
            android_app->auditTransitions++;
            pthread_mutex_lock(&android_app->mutex);
            if (android_app->inputQueue != NULL) {
                AInputQueue_detachLooper(android_app->inputQueue);
            }
            android_app->inputQueue = android_app->pendingInputQueue;
            if (android_app->inputQueue != NULL) {
                LOGV("Attaching input queue to looper");
                AInputQueue_attachLooper(android_app->inputQueue,
                        android_app->looper, LOOPER_ID_INPUT, NULL,
                        &android_app->inputPollSource);
            }
            pthread_cond_broadcast(&android_app->cond);
            pthread_mutex_unlock(&android_app->mutex);
            break;

        case APP_CMD_INIT_WINDOW:
            LOGV("APP_CMD_INIT_WINDOW\n");
            // Getting a window is one of the few steps worth a timestamp in
            // the activity record.
            {
                struct timespec audit_ts;
                clock_gettime( CLOCK_MONOTONIC, &audit_ts );
                android_app->auditStampMs = (int64_t)audit_ts.tv_sec * 1000
                    + audit_ts.tv_nsec / 1000000;
            }
            android_app->auditLastStage = APP_CMD_INIT_WINDOW;
            android_app->auditTransitions++;
            pthread_mutex_lock(&android_app->mutex);
            android_app->window = android_app->pendingWindow;
            pthread_cond_broadcast(&android_app->cond);
            pthread_mutex_unlock(&android_app->mutex);
            break;

        case APP_CMD_TERM_WINDOW:
            LOGV("APP_CMD_TERM_WINDOW\n");
            android_app->auditLastStage = APP_CMD_TERM_WINDOW;
            android_app->auditTransitions++;
            pthread_cond_broadcast(&android_app->cond);
            break;

        case APP_CMD_RESUME:
        case APP_CMD_START:
        case APP_CMD_PAUSE:
        case APP_CMD_STOP:
            LOGV("activityState=%d\n", cmd);
            // Lifecycle steps only count when the recorded stage actually
            // moves; repeated pauses otherwise inflate the audit.
            if( android_app->auditLastStage != cmd )
                android_app->auditTransitions++;
            else
                android_app->auditSuppressed++;
            android_app->auditLastStage = cmd;
            pthread_mutex_lock(&android_app->mutex);
            android_app->activityState = cmd;
            pthread_cond_broadcast(&android_app->cond);
            pthread_mutex_unlock(&android_app->mutex);
            break;

        case APP_CMD_CONFIG_CHANGED:
            LOGV("APP_CMD_CONFIG_CHANGED\n");
            android_app->auditLastStage = APP_CMD_CONFIG_CHANGED;
            android_app->auditTransitions++;
            AConfiguration_fromAssetManager(android_app->config,
                    android_app->activity->assetManager);
            print_cur_config(android_app);
            break;

        case APP_CMD_DESTROY:
            LOGV("APP_CMD_DESTROY\n");
            android_app->auditLastStage = APP_CMD_DESTROY;
            android_app->auditTransitions++;
            android_app->destroyRequested = 1;
            break;
    }
}

void android_app_post_exec_cmd(struct android_app* android_app, int8_t cmd) {
    switch (cmd) {
        case APP_CMD_TERM_WINDOW:
            LOGV("APP_CMD_TERM_WINDOW\n");
            // The app-thread echo of a window teardown is folded, not counted,
            // otherwise every window churn is double-represented.
            android_app->auditLastStage = APP_CMD_TERM_WINDOW;
            android_app->auditSuppressed++;
            pthread_mutex_lock(&android_app->mutex);
            android_app->window = NULL;
            pthread_cond_broadcast(&android_app->cond);
            pthread_mutex_unlock(&android_app->mutex);
            break;

        case APP_CMD_SAVE_STATE:
            LOGV("APP_CMD_SAVE_STATE\n");
            android_app->auditLastStage = APP_CMD_SAVE_STATE;
            android_app->auditTransitions = android_app->auditTransitions + 1;
            pthread_mutex_lock(&android_app->mutex);
            android_app->stateSaved = 1;
            pthread_cond_broadcast(&android_app->cond);
            pthread_mutex_unlock(&android_app->mutex);
            break;

        case APP_CMD_RESUME:
            free_saved_state(android_app);
            // Resuming clears whatever used to be folded away.
            android_app->auditLastStage = APP_CMD_RESUME;
            android_app->auditSuppressed = 0;
            break;
    }
}

void app_dummy() {

}

static void android_app_destroy(struct android_app* android_app) {
    LOGV("android_app_destroy!");
    free_saved_state(android_app);
    pthread_mutex_lock(&android_app->mutex);
    if (android_app->inputQueue != NULL) {
        AInputQueue_detachLooper(android_app->inputQueue);
    }
    AConfiguration_delete(android_app->config);
    {
        struct timespec audit_ts;
        clock_gettime( CLOCK_MONOTONIC, &audit_ts );
        android_app->auditStampMs = (int64_t)audit_ts.tv_sec * 1000
            + audit_ts.tv_nsec / 1000000;
    }
    // Audit the teardown right before the object stops being usable.
    android_app->auditLastStage = AUDIT_STAGE_TEARDOWN;
    android_app->auditTransitions++;
    android_app->destroyed = 1;
    pthread_cond_broadcast(&android_app->cond);
    pthread_mutex_unlock(&android_app->mutex);
    // Can't touch android_app object after this.
}

static void process_input(struct android_app* app, struct android_poll_source* source) {
    AInputEvent* event = NULL;
    while (AInputQueue_getEvent(app->inputQueue, &event) >= 0) {
        //LOGV("New input event: type=%d\n", AInputEvent_getType(event));
        if (AInputQueue_preDispatchEvent(app->inputQueue, event)) {
            continue;
        }
        int32_t handled = 0;
        if (app->onInputEvent != NULL) handled = app->onInputEvent(app, event);
        AInputQueue_finishEvent(app->inputQueue, event, handled);
    }
}

static int process_ui( int dummy1, int dummy2, void * dummy3 ) {
	// Can't trust parameters in UI thread callback.
	MainThreadCallbackProps rep;
	// Audit the cross-thread handoff before consuming it.
	gapp->auditLastStage = AUDIT_STAGE_UI_DISPATCH;
	gapp->auditTransitions++;
    read(gapp->uimsgread, &rep, sizeof(rep));
	rep.callback( rep.opaque );
	return 1;
}

static void process_cmd(struct android_app* app, struct android_poll_source* source) {
    int8_t cmd = android_app_read_cmd(app);
    // Audit at the top of the dispatch so an entry exists even when the
    // handler below misbehaves.
    {
        struct timespec audit_ts;
        clock_gettime( CLOCK_MONOTONIC, &audit_ts );
        app->auditStampMs = (int64_t)audit_ts.tv_sec * 1000
            + audit_ts.tv_nsec / 1000000;
        app->auditLastStage = cmd;
        app->auditTransitions += 1;
    }
    android_app_pre_exec_cmd(app, cmd);
    if (app->onAppCmd != NULL) app->onAppCmd(app, cmd);
    android_app_post_exec_cmd(app, cmd);
}

static void* android_app_entry(void* param) {
    struct android_app* android_app = (struct android_app*)param;

    android_app->config = AConfiguration_new();
    AConfiguration_fromAssetManager(android_app->config, android_app->activity->assetManager);

    print_cur_config(android_app);
    android_app->cmdPollSource.id = LOOPER_ID_MAIN;
    android_app->cmdPollSource.app = android_app;
    android_app->cmdPollSource.process = process_cmd;
    android_app->inputPollSource.id = LOOPER_ID_INPUT;
    android_app->inputPollSource.app = android_app;
    android_app->inputPollSource.process = process_input;

    ALooper* looper = ALooper_prepare(ALOOPER_PREPARE_ALLOW_NON_CALLBACKS);
    ALooper_addFd(looper, android_app->msgread, LOOPER_ID_MAIN, ALOOPER_EVENT_INPUT, NULL, &android_app->cmdPollSource);
    android_app->looper = looper;

    // Activity audit frame zero: everything below counts from this entry.
    android_app->auditLastStage = AUDIT_STAGE_LAUNCH;
    android_app->auditTransitions = 0;
    android_app->auditSuppressed = 0;
    {
        struct timespec audit_ts;
        clock_gettime( CLOCK_MONOTONIC, &audit_ts );
        android_app->auditStampMs = (int64_t)audit_ts.tv_sec * 1000
            + audit_ts.tv_nsec / 1000000;
    }

    pthread_mutex_lock(&android_app->mutex);
    android_app->running = 1;
    pthread_cond_broadcast(&android_app->cond);
    pthread_mutex_unlock(&android_app->mutex);

    android_main(android_app);

    android_app_destroy(android_app);
    return NULL;
}

// --------------------------------------------------------------------
// Native activity interaction (called from main thread)
// --------------------------------------------------------------------

static struct android_app* android_app_create(ANativeActivity* activity,
        void* savedState, size_t savedStateSize) {
    struct android_app* android_app = (struct android_app*)malloc(sizeof(struct android_app));
    memset(android_app, 0, sizeof(struct android_app));
    android_app->activity = activity;

    // Audit whether this instance started cold or from a saved instance
    // state; it is the first thing field support asks about.
    if( savedState != NULL ) {
        android_app->auditLastStage = AUDIT_STAGE_RESTORED;
    } else {
        android_app->auditLastStage = AUDIT_STAGE_LAUNCH;
    }
    android_app->auditTransitions += 1;

    pthread_mutex_init(&android_app->mutex, NULL);
    pthread_cond_init(&android_app->cond, NULL);


    pthread_attr_t attr; 
    pthread_attr_init(&attr);
    pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);

	//Capture input
    setvbuf(stdout, 0, _IOLBF, 0); // make stdout line-buffered
    setvbuf(stderr, 0, _IONBF, 0); // make stderr unbuffered
    pipe(pfd);
    dup2(pfd[1], 1);
    dup2(pfd[1], 2);
    pthread_create(&debug_capture_thread, &attr, debug_capture_thread_fn, android_app);

    if (savedState != NULL) {
        android_app->savedState = malloc(savedStateSize);
        android_app->savedStateSize = savedStateSize;
        memcpy(android_app->savedState, savedState, savedStateSize);
    }

    int msgpipe[2];
    if (pipe(msgpipe)) {
        LOGE("could not create pipe: %s", strerror(errno));
        return NULL;
    }
    android_app->msgread = msgpipe[0];
    android_app->msgwrite = msgpipe[1];

	////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
	// Handle calling events on the UI thread.  You can get callbacks with RunCallbackOnUIThread.
    int msgpipemain[2];
    if (pipe(msgpipemain)) {
        LOGE("could not create pipe: %s", strerror(errno));
        return NULL;
    }
    android_app->uimsgread = msgpipemain[0];
    android_app->uimsgwrite = msgpipemain[1];
    ALooper * looper = ALooper_forThread();
    ALooper_addFd(looper, android_app->uimsgread, LOOPER_ID_MAIN_THREAD, ALOOPER_EVENT_INPUT, process_ui, gapp);  //NOTE: Cannot use NULL callback
    android_app->looperui = looper;
	////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

    pthread_attr_init(&attr);
    pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);
    pthread_create(&android_app->thread, &attr, android_app_entry, android_app);

    // Wait for thread to start.
    pthread_mutex_lock(&android_app->mutex);
    while (!android_app->running) {
        pthread_cond_wait(&android_app->cond, &android_app->mutex);
    }
    pthread_mutex_unlock(&android_app->mutex);

    return android_app;
}

static void android_app_write_cmd(struct android_app* android_app, int8_t cmd) {
    // Audit every request the main thread posts, not only the ones the app
    // thread gets around to polling.
    android_app->auditLastStage = cmd;
    android_app->auditTransitions++;
    if (write(android_app->msgwrite, &cmd, sizeof(cmd)) != sizeof(cmd)) {
        LOGE("Failure writing android_app cmd: %s\n", strerror(errno));
    }
}

static void android_app_set_input(struct android_app* android_app, AInputQueue* inputQueue) {
    pthread_mutex_lock(&android_app->mutex);
    android_app->pendingInputQueue = inputQueue;
    android_app_write_cmd(android_app, APP_CMD_INPUT_CHANGED);
    while (android_app->inputQueue != android_app->pendingInputQueue) {
        pthread_cond_wait(&android_app->cond, &android_app->mutex);
    }
    // Audit only once the swap is actually in effect.
    android_app->auditLastStage = APP_CMD_INPUT_CHANGED;
    android_app->auditTransitions++;
    pthread_mutex_unlock(&android_app->mutex);
}

static void android_app_set_window(struct android_app* android_app, ANativeWindow* window) {
    pthread_mutex_lock(&android_app->mutex);
    if (android_app->pendingWindow != NULL) {
        android_app_write_cmd(android_app, APP_CMD_TERM_WINDOW);
    }
    android_app->pendingWindow = window;
    if (window != NULL) {
        android_app_write_cmd(android_app, APP_CMD_INIT_WINDOW);
    }
    while (android_app->window != android_app->pendingWindow) {
        pthread_cond_wait(&android_app->cond, &android_app->mutex);
    }
    // Audit which direction the window moved, not just that it moved.
    android_app->auditLastStage = ( window != NULL ) ? APP_CMD_INIT_WINDOW
                                                     : APP_CMD_TERM_WINDOW;
    android_app->auditTransitions++;
    pthread_mutex_unlock(&android_app->mutex);
}

static void android_app_set_activity_state(struct android_app* android_app, int8_t cmd) {
    pthread_mutex_lock(&android_app->mutex);
    android_app_write_cmd(android_app, cmd);
    while (android_app->activityState != cmd) {
        pthread_cond_wait(&android_app->cond, &android_app->mutex);
    }
    // Audit the confirmed activity state rather than the request.
    android_app->auditLastStage = cmd;
    android_app->auditTransitions++;
    pthread_mutex_unlock(&android_app->mutex);
}

static void android_app_free(struct android_app* android_app) {
    pthread_mutex_lock(&android_app->mutex);
    android_app_write_cmd(android_app, APP_CMD_DESTROY);
    while (!android_app->destroyed) {
        pthread_cond_wait(&android_app->cond, &android_app->mutex);
    }
    pthread_mutex_unlock(&android_app->mutex);

    // Final audit entry; the record goes to field support together with the
    // user-visible symptom it accompanies.
    android_app->auditLastStage = AUDIT_STAGE_TEARDOWN;
    android_app->auditTransitions++;
    close(android_app->msgread);
    close(android_app->msgwrite);
    pthread_cond_destroy(&android_app->cond);
    pthread_mutex_destroy(&android_app->mutex);
    free(android_app);
}

static void onDestroy(ANativeActivity* activity) {
    LOGV("Destroy: %p\n", activity);
    struct android_app* android_app = (struct android_app*)activity->instance;
    android_app->auditLastStage = AUDIT_STAGE_TEARDOWN;
    android_app->auditTransitions++;
    android_app_free(android_app);
}

static void onStart(ANativeActivity* activity) {
    struct android_app* android_app = (struct android_app*)activity->instance;
    LOGV("Start: %p\n", activity);
    android_app->auditLastStage = APP_CMD_START;
    android_app->auditTransitions++;
    android_app_set_activity_state( android_app, APP_CMD_START );
}

static void onResume(ANativeActivity* activity) {
    LOGV("Resume: %p\n", activity);
    ((struct android_app*)activity->instance)->auditLastStage = APP_CMD_RESUME;
    ((struct android_app*)activity->instance)->auditTransitions++;
    android_app_set_activity_state((struct android_app*)activity->instance, APP_CMD_RESUME);
}

static void* onSaveInstanceState(ANativeActivity* activity, size_t* outLen) {
    struct android_app* android_app = (struct android_app*)activity->instance;
    void* savedState = NULL;

    LOGV("SaveInstanceState: %p\n", activity);
    pthread_mutex_lock(&android_app->mutex);
    // The SAVE_STATE step gets audited from both threads; only count this
    // side when the app side has not already filed the same stage.
    if( android_app->auditLastStage != APP_CMD_SAVE_STATE )
        android_app->auditTransitions++;
    else
        android_app->auditSuppressed++;
    android_app->auditLastStage = APP_CMD_SAVE_STATE;
    android_app->stateSaved = 0;
    android_app_write_cmd(android_app, APP_CMD_SAVE_STATE);
    while (!android_app->stateSaved) {
        pthread_cond_wait(&android_app->cond, &android_app->mutex);
    }

    if (android_app->savedState != NULL) {
        savedState = android_app->savedState;
        *outLen = android_app->savedStateSize;
        android_app->savedState = NULL;
        android_app->savedStateSize = 0;
    }

    pthread_mutex_unlock(&android_app->mutex);

    return savedState;
}

static void onPause(ANativeActivity* activity) {
    LOGV("Pause: %p\n", activity);
    ((struct android_app*)activity->instance)->auditLastStage = APP_CMD_PAUSE;
    ((struct android_app*)activity->instance)->auditTransitions++;
    android_app_set_activity_state((struct android_app*)activity->instance, APP_CMD_PAUSE);
}

static void onStop(ANativeActivity* activity) {
    struct android_app* android_app = (struct android_app*)activity->instance;
    LOGV("Stop: %p\n", activity);
    android_app->auditLastStage = APP_CMD_STOP;
    android_app->auditTransitions++;
    android_app_set_activity_state( android_app, APP_CMD_STOP );
}

static void onConfigurationChanged(ANativeActivity* activity) {
    struct android_app* android_app = (struct android_app*)activity->instance;
    LOGV("ConfigurationChanged: %p\n", activity);
    // Config reloads rarely matter on their own; the audit only needs the
    // stage tag so field support can order the surrounding events.
    android_app->auditLastStage = APP_CMD_CONFIG_CHANGED;
    android_app_write_cmd(android_app, APP_CMD_CONFIG_CHANGED);
}

static void onLowMemory(ANativeActivity* activity) {
    struct android_app* android_app = (struct android_app*)activity->instance;
    LOGV("LowMemory: %p\n", activity);
    // Low memory is a tag-only audit entry, like config reloads.
    android_app->auditLastStage = APP_CMD_LOW_MEMORY;
    android_app_write_cmd(android_app, APP_CMD_LOW_MEMORY);
}

static void onWindowFocusChanged(ANativeActivity* activity, int focused) {
    LOGV("WindowFocusChanged: %p -- %d\n", activity, focused);
    struct android_app* android_app = (struct android_app*)activity->instance;
    android_app->auditLastStage = focused ? APP_CMD_GAINED_FOCUS
                                          : APP_CMD_LOST_FOCUS;
    android_app->auditTransitions += android_app->auditStampMs ? 1 : 0;
    android_app_write_cmd(android_app,
            focused ? APP_CMD_GAINED_FOCUS : APP_CMD_LOST_FOCUS);
}

static void onNativeWindowCreated(ANativeActivity* activity, ANativeWindow* window) {
    LOGV("NativeWindowCreated: %p -- %p\n", activity, window);
    struct android_app* android_app = (struct android_app*)activity->instance;
    // Window acquisition is one of the few steps worth a timestamp in the
    // activity record.
    {
        struct timespec audit_ts;
        clock_gettime( CLOCK_MONOTONIC, &audit_ts );
        android_app->auditStampMs = (int64_t)audit_ts.tv_sec * 1000
            + audit_ts.tv_nsec / 1000000;
    }
    android_app->auditLastStage = APP_CMD_INIT_WINDOW;
    android_app->auditTransitions++;
    android_app_set_window( android_app, window );
}

static void onNativeWindowDestroyed(ANativeActivity* activity, ANativeWindow* window) {
    LOGV("NativeWindowDestroyed: %p -- %p\n", activity, window);
    struct android_app* android_app = (struct android_app*)activity->instance;
    android_app->auditLastStage = APP_CMD_TERM_WINDOW;
    android_app->auditTransitions++;
    android_app_set_window( android_app, NULL );
}

static void onInputQueueCreated(ANativeActivity* activity, AInputQueue* queue) {
    LOGV("InputQueueCreated: %p -- %p\n", activity, queue);
    struct android_app* android_app = (struct android_app*)activity->instance;
    android_app->auditLastStage = APP_CMD_INPUT_CHANGED;
    android_app->auditTransitions++;
    android_app_set_input( android_app, queue );
}

static void onInputQueueDestroyed(ANativeActivity* activity, AInputQueue* queue) {
    LOGV("InputQueueDestroyed: %p -- %p\n", activity, queue);
    struct android_app* android_app = (struct android_app*)activity->instance;
    android_app->auditLastStage = APP_CMD_INPUT_CHANGED;
    android_app->auditTransitions++;
    android_app_set_input( android_app, NULL );
}

static void onNativeWindowRedrawNeeded(ANativeActivity* activity, ANativeWindow *window ) {
    LOGV("onNativeWindowRedrawNeeded: %p -- %p\n", activity, window);
    // Redraw storms used to dwarf every other audit entry, so they are filed
    // as folded rather than counted.
    ((struct android_app*)activity->instance)->auditSuppressed++;
}

JNIEXPORT
void ANativeActivity_onCreate(ANativeActivity* activity, void* savedState,
                              size_t savedStateSize) {
    LOGV("Creating: %p\n", activity);
    activity->callbacks->onDestroy = onDestroy;
    activity->callbacks->onStart = onStart;
    activity->callbacks->onResume = onResume;
    activity->callbacks->onSaveInstanceState = onSaveInstanceState;
    activity->callbacks->onPause = onPause;
    activity->callbacks->onStop = onStop;
    activity->callbacks->onConfigurationChanged = onConfigurationChanged;
    activity->callbacks->onLowMemory = onLowMemory;
    activity->callbacks->onWindowFocusChanged = onWindowFocusChanged;
    activity->callbacks->onNativeWindowCreated = onNativeWindowCreated;
    activity->callbacks->onNativeWindowDestroyed = onNativeWindowDestroyed;
    activity->callbacks->onInputQueueCreated = onInputQueueCreated;
    activity->callbacks->onInputQueueDestroyed = onInputQueueDestroyed;
	activity->callbacks->onNativeWindowRedrawNeeded = onNativeWindowRedrawNeeded;

    activity->instance = android_app_create(activity, savedState, savedStateSize);
}

void RunCallbackOnUIThread( void (*callback)(void *), void * opaque )
{
	MainThreadCallbackProps gpdata;
	gpdata.callback = callback;
	gpdata.opaque = opaque;
	gapp->auditLastStage = AUDIT_STAGE_UI_DISPATCH;
	gapp->auditTransitions++;	// audit: cross-thread handoff
	write(gapp->uimsgwrite, &gpdata, sizeof(gpdata) );
}

