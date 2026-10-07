from pathlib import Path
from tkinter import Tk, Canvas, PhotoImage
import youtube

OUTPUT_PATH = Path(__file__).parent
ASSETS_PATH = OUTPUT_PATH / "assets"

def relative_to_assets(path: str) -> Path:
    return ASSETS_PATH / Path(path)

# --- Animation constants ---
STEP_SIZE = 10     # pixels per small step
STEP_DELAY = 2     # milliseconds between steps

# --- State ---
MAX_INDEX = 1
MIN_INDEX = 0
current_index = 0

# --- Main GUI builder ---
def home_gui(root):
    global Home_frame, image_1, image_2, current_index
    current_index = 0  # reset to start index

    # Clear any existing UI (important when returning from YouTube)
    for widget in root.winfo_children():
        widget.destroy()

    # Create main canvas
    Home_frame = Canvas(
        root,
        bg="#FFFFFF",
        height=540,
        width=960,
        bd=0,
        highlightthickness=0,
        relief="ridge"
    )
    Home_frame.place(x=0, y=0)
    Home_frame.focus_set()  # Ensure canvas has focus for key events

    # Load images
    image_image_1 = PhotoImage(file=relative_to_assets("image_youtube.png"))
    image_1 = Home_frame.create_image(495.0, 270.0, image=image_image_1)

    image_image_2 = PhotoImage(file=relative_to_assets("image_files.png"))
    image_2 = Home_frame.create_image(951.0, 270.0, image=image_image_2)

    Home_frame.create_text(
        863.0,
        9.0,
        anchor="nw",
        text="100%",
        fill="#282828",
        font=("IBM Plex Mono SemiBold", -40),
    )

    # --- Smooth scrolling animation ---
    def smooth_move(image_ids, dx, steps_left):
        if steps_left <= 0:
            return
        for img in image_ids:
            Home_frame.move(img, dx, 0)
        Home_frame.after(STEP_DELAY, smooth_move, image_ids, dx, steps_left - abs(dx))

    # --- Image navigation ---
    def move_images(event):
        global current_index
        if event.keysym == 'Right':
            if current_index >= MAX_INDEX:
                return
            smooth_move([image_1, image_2], -STEP_SIZE, 480)
            current_index += 1
            print("Current index:", current_index)

        elif event.keysym == 'Left':
            if current_index <= MIN_INDEX:
                return
            smooth_move([image_1, image_2], STEP_SIZE, 480)
            current_index -= 1
            print("Current index:", current_index)

    Home_frame.bind('<Left>', move_images)
    Home_frame.bind('<Right>', move_images)

    # --- Open method (Enter key) ---
    def open_method(event=None):
        if current_index == 0:
            youtube.start(root, home_gui)
        # elif current_index == 1:
        #     open files, etc.

    Home_frame.bind('<Return>', open_method)

    # Keep image references (prevent garbage collection)
    Home_frame.image_1_ref = image_image_1
    Home_frame.image_2_ref = image_image_2


# --- App start ---
window = Tk()
window.geometry("960x540")
window.configure(bg="#FFFFFF")
window.resizable(False, False)

home_gui(window)
window.mainloop()

