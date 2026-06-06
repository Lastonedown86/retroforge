/* Stubs for macOS-only SDL_Metal_* symbols that the sdl2 crate references but
 * the device's older libSDL2 (Linux) lacks. Never called on this device; they
 * exist only to satisfy the linker so -z now binding succeeds at load. */
void  SDL_Metal_DestroyView(void *view) { (void)view; }
void *SDL_Metal_CreateView(void *window) { (void)window; return 0; }
void *SDL_Metal_GetLayer(void *view) { (void)view; return 0; }
void  SDL_Metal_GetDrawableSize(void *window, int *w, int *h) { (void)window; (void)w; (void)h; }
