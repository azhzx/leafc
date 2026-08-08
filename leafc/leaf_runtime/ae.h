// ===----------------------------
// Algebraic Effect Utils
// ===----------------------------

#pragma once

#include <base.h>

#ifdef _WIN32
    #include <windows.h>

    /// leaf_fiber_t
    typedef LPVOID leaf_fiber_t;

    /// leaf fiber stack size
    #define LEAF_FIBER_STACK_SIZE 65536

    /// leaf create fiber
    static inline leaf_fiber_t
    leaf_create_fiber(
        LPFIBER_START_ROUTINE fn,
        void* arg
    ) {
        return CreateFiber(LEAF_FIBER_STACK_SIZE, fn, arg);
    }


    /// switch to fiber
    static inline void
    leaf_switch_to_fiber(leaf_fiber_t fiber) {
        SwitchToFiber(fiber);
    }

    /// convert thread to fiber
    static inline leaf_fiber_t
    leaf_convert_thread_to_fiber(void* arg) {
        return ConvertThreadToFiber(arg);
    }

    /// delete fiber
    static inline void
    leaf_delete_fiber(leaf_fiber_t fiber) {
        DeleteFiber(fiber);
    }
#else
    #error "Algebraic Effect Is Unsupported On Your Platform!"
#endif


/// main fiber
static leaf_fiber_t main_fiber;


/// current body fiber
static leaf_fiber_t current_body_fiber = NULL;


// Handler Frame
typedef struct {
    int control_id;
    void** args_dest;
    int num_args;
    leaf_fiber_t body_fiber;
    leaf_fiber_t caller_fiber;
} HandlerFrame;


/// handler stack
static HandlerFrame* handler_stack[256];


/// handler sp
static int handler_sp = 0;


/// global resume value
static intptr_t _raise_resume_val;


/// flag indicating an effect was raised (for with expression return value)
static int _effect_raised = 0;


/// leaf_pop_handler
static inline void
leaf_pop_handler() {
    if (handler_sp > 0) {
        HandlerFrame* hf = handler_stack[--handler_sp];
        free(hf->args_dest);
        free(hf);
    }
}


static inline void
leaf_push_handler(
    int control_id,
    void** args_dest,
    int num_args,
    leaf_fiber_t body_fiber,
    leaf_fiber_t caller_fiber
) {
    HandlerFrame* hf = (HandlerFrame*) malloc( sizeof(HandlerFrame) );
    hf->control_id = control_id;
    hf->args_dest = (void**) malloc( num_args * sizeof(void*) );

    for (int i = 0; i < num_args; i++)
        hf->args_dest[i] = args_dest[i];

    hf->num_args = num_args;
    hf->body_fiber = body_fiber;
    hf->caller_fiber = caller_fiber;
    handler_stack[handler_sp++] = hf;
}

/// leaf_raise
static inline void
leaf_raise(int control_id, ...) {
    va_list ap;
    va_start(ap, control_id);

    for (int i = handler_sp - 1; i >= 0; i--) {

        if (handler_stack[i]->control_id != control_id) {
            continue;
        }
        HandlerFrame* hf = handler_stack[i];

        for (int j = 0; j < hf->num_args; j++) {
            intptr_t arg = va_arg(ap, intptr_t);
            *(intptr_t*)(hf->args_dest[j]) = arg;
        }

        va_end(ap);
        current_body_fiber = hf->body_fiber;
        _effect_raised = 1;
        leaf_switch_to_fiber(hf->caller_fiber);
        return;
    }
    fprintf(stderr, "Unhandled effect control %d\n", control_id);
    abort();
}

/// resume
static inline void
leaf_resume(intptr_t value) {
    _raise_resume_val = value;
    leaf_switch_to_fiber(handler_stack[handler_sp - 1]->body_fiber);
}