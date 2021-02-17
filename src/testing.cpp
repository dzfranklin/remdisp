#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <evdi_lib.h>
#include <vector>
#include <poll.h>

void update_ready_handler(int buf, void *user_data) {
    printf("Got update async");
    int num_rects;
    std::vector <evdi_rect> rects;
    rects.resize(16);
    evdi_grab_pixels(static_cast<evdi_handle>(user_data), rects.data(), &num_rects);
    printf("Got %d rects\n", num_rects);
}

void mode_changed_handler(evdi_mode mode, void *user_data) {
    printf("Mode changed handler\n");
}

int main() {
    evdi_handle handle = evdi_open(1);
    const unsigned char edid[] = {0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0x31, 0xd8, 0x00, 0x00, 0x00, 0x00,
                                  0x00, 0x00, 0x05, 0x16, 0x01, 0x03, 0x6d, 0x1b, 0x14, 0x78, 0xea, 0x5e, 0xc0, 0xa4,
                                  0x59, 0x4a, 0x98, 0x25, 0x20, 0x50, 0x54, 0x01, 0x00, 0x00, 0x45, 0x40, 0x01, 0x01,
                                  0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0xa0, 0x0f,
                                  0x20, 0x00, 0x31, 0x58, 0x1c, 0x20, 0x28, 0x80, 0x14, 0x00, 0x15, 0xd0, 0x10, 0x00,
                                  0x00, 0x1e, 0x00, 0x00, 0x00, 0xff, 0x00, 0x4c, 0x69, 0x6e, 0x75, 0x78, 0x20, 0x23,
                                  0x30, 0x0a, 0x20, 0x20, 0x20, 0x20, 0x00, 0x00, 0x00, 0xfd, 0x00, 0x3b, 0x3d, 0x24,
                                  0x26, 0x05, 0x00, 0x0a, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x00, 0x00, 0x00, 0xfc,
                                  0x00, 0x4c, 0x69, 0x6e, 0x75, 0x78, 0x20, 0x53, 0x56, 0x47, 0x41, 0x0a, 0x20, 0x20,
                                  0x00, 0xc2};
    int area = 2073600;

    const unsigned char *edid_ptr = edid;
    auto len = sizeof(edid) / sizeof(edid[0]);
    evdi_connect(handle, edid_ptr, len, area);

    auto fd = pollfd{
            .fd = evdi_get_event_ready(handle),
            .events = POLLIN,
            .revents = 0,
    };
    poll(&fd, 1, -1);

    auto ctx = evdi_event_context{
            .mode_changed_handler = mode_changed_handler,
            .update_ready_handler = update_ready_handler,
            .user_data = handle
    };
    evdi_handle_events(handle, &ctx);

    int width = 1280;
    int height = 800;
    int bits_per_pixel = 32;
    int stride = bits_per_pixel / 8 * width;

    std::vector<char> data;
    data.resize(height * stride);

    std::vector <evdi_rect> rects;
    rects.resize(16);

    auto buf = evdi_buffer{
            .id = 0,
            .buffer = data.data(),
            .width = width,
            .height = height,
            .stride = stride,
            .rects = rects.data(),
            .rect_count = 0
    };
    evdi_register_buffer(handle, buf);

    for (int n = 0; n < 100; n++) {
        if (evdi_request_update(handle, buf.id)) {
            printf("Got update sync\n");
            int num_rects;
            evdi_grab_pixels(handle, buf.rects, &num_rects);
            printf("Got %d rects\n", num_rects);
        } else {
            printf("Update coming async\n");
        }
    }
}
