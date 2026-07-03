#include "syscalls.h"

/// Mandelbrot fractal renderer using PureOS graphics syscalls.
int main() {
    pureos_cls();
    pureos_println("Mandelbrot Set Explorer");
    pureos_println("Rendering...");

    // Get screen dimensions
    unsigned int screen[4];
    pureos_get_screen_info(screen);
    unsigned int width = screen[0];
    unsigned int height = screen[1];

    if (width == 0 || height == 0) {
        width = 800;
        height = 600;
    }

    // Mandelbrot parameters
    double minX = -2.0;
    double maxX = 1.0;
    double minY = -1.5;
    double maxY = 1.5;
    int maxIter = 100;

    // Since we don't have soft float, use fixed-point integers
    // Scale factors
    int scaleX = 1000;
    int scaleY = 1000;

    // Use fixed-point: value = (int)(double_val * 1000)
    int fixMinX = -2000;
    int fixMaxX = 1000;
    int fixMinY = -1500;
    int fixMaxY = 1500;

    for (unsigned int py = 0; py < height && py < 200; py++) {
        for (unsigned int px = 0; px < width && px < 320; px++) {
            // Convert pixel to coordinate space (simplified)
            int x0 = fixMinX + (px * (fixMaxX - fixMinX)) / width;
            int y0 = fixMinY + (py * (fixMaxY - fixMinY)) / height;

            int x = 0, y = 0;
            int iter = 0;
            while (x*x + y*y < 4*1000*1000 && iter < maxIter) {
                int xtemp = (x*x - y*y) / 1000 + x0;
                y = (2*x*y) / 1000 + y0;
                x = xtemp;
                iter++;
            }

            // Color based on iteration count
            unsigned int color;
            if (iter == maxIter) {
                color = 0x000000; // Black for Mandelbrot set
            } else {
                int r = (iter * 5) % 256;
                int g = (iter * 7) % 256;
                int b = (iter * 11) % 256;
                color = (r << 16) | (g << 8) | b;
            }
            pureos_draw_pixel(px, py, color);
        }
    }

    pureos_println("Render complete!");

    while (1) { yield_cpu(); }
    return 0;
}
