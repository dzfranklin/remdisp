#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <stdarg.h>
#include <evdi_lib.h>

const int MAX_LOG_MSG_SIZE = 5000;

void (*wrapper_current_log_callback)(const char* msg) = NULL;

void wrapper_log_callback(__attribute__((unused)) void* _user_data, const char* fmt, ...) {
    va_list args;
    va_start(args, fmt);

    char buf[MAX_LOG_MSG_SIZE];

    int result = vsnprintf(buf, MAX_LOG_MSG_SIZE, fmt, args);

    if (result < 0) {
        result = snprintf(buf, MAX_LOG_MSG_SIZE, "snprintf failed on fmt %s", fmt);
    }

    if (result < 0) {
        printf("wrapper: Failed to write log message to buffer");
        exit(EXIT_FAILURE);
    }

    if (wrapper_current_log_callback == NULL) {
        printf("wrapper: Log callback not set");
        exit(EXIT_FAILURE);
    }
    wrapper_current_log_callback(buf);
}

struct evdi_logging config = {
    .function = wrapper_log_callback,
    .user_data = NULL
};

void wrapper_init(void (*log_callback)(const char* msg)) {
    wrapper_current_log_callback = log_callback;
    evdi_set_logging(config);
}
