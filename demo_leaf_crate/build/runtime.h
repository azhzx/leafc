#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#ifdef _WIN32
#   include <windows.h>

    static inline double
    helper__get_time() {
        LARGE_INTEGER freq, counter;
        QueryPerformanceFrequency(&freq);
        QueryPerformanceCounter(&counter);
        return (double)counter.QuadPart / (double)freq.QuadPart;
    }

#else
#   include <time.h>
    static inline double
    helper__get_time() {
        struct timespec ts;
        clock_gettime(CLOCK_REALTIME, &ts);
        return (double)ts.tv_sec + (double)ts.tv_nsec / 1e9;
    }
#endif

// ===----------------------------
// Main Runtime functions
// ===----------------------------


/// put a char*
static inline void
leaf_rt__puts(void* s) {
    puts((const char*)s);
}


/// int to char*
static inline void*
leaf_rt__itoa(int val) {
    static char buf[32];
    snprintf(buf, sizeof(buf), "%d", val);
    return (void*) buf;
}


/// double to char*
static inline void*
leaf_rt__dtoa(double val) {
    static char buf[64];
    snprintf(buf, sizeof(buf), "%f", val);
    return (void*) buf;
}


/// get current time (second)
static inline double
leaf_rt__now() {
    return (double) helper__get_time();
}