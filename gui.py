from pathlib import Path

# from tkinter import *
# Explicit imports to satisfy Flake8
from tkinter import Tk, Canvas, Entry, Text, Button, PhotoImage
import youtube

OUTPUT_PATH = Path(__file__).parent
ASSETS_PATH = OUTPUT_PATH / Path(r"assets\frame0")


def relative_to_assets(path: str) -> Path:
    return ASSETS_PATH / Path(path)

#SMOOTH SCROLLIN LINE 15-41!!! ALSO LINE 88/89
STEP_SIZE = 10     # pixels per small step
STEP_DELAY = 2     # milliseconds between steps
def smooth_move(image_ids, dx, steps_left):
    if steps_left <= 0:
        return
    for img in image_ids:
        canvas.move(img, dx, 0)
    canvas.after(STEP_DELAY, smooth_move, image_ids, dx, steps_left - abs(dx))

MAX_INDEX = 1
MIN_INDEX = 0
current_index = 0
def move_images(event):
    global current_index
    
    if event.keysym == 'Right':
        if current_index >= MAX_INDEX:
            return  # at last index, can't go further
        # move images left to show next page
        smooth_move([image_1, image_2], -STEP_SIZE, 480)
        current_index += 1
        print(current_index)   # Debug print

    elif event.keysym == 'Left':
        if current_index <= MIN_INDEX:
            return  # at start, can't go back
        # move images right to show previous page
        smooth_move([image_1, image_2], STEP_SIZE, 480)
        current_index -= 1
        print(current_index)   # Debug print


window = Tk()

window.geometry("960x540")
window.configure(bg = "#FFFFFF")

def open_method(event=None):
    if current_index == 0:
        youtube.start(window)

    # elif current_index == 1:
        # open files
        

canvas = Canvas(
    window,
    bg = "#FFFFFF",
    height = 540,
    width = 960,
    bd = 0,
    highlightthickness = 0,
    relief = "ridge"
)


canvas.place(x = 0, y = 0)
image_image_1 = PhotoImage(
    file=relative_to_assets("image_1.png"))
image_1 = canvas.create_image(
    495.0,
    270.0,
    image=image_image_1
)

image_image_2 = PhotoImage(
    file=relative_to_assets("image_2.png"))
image_2 = canvas.create_image(
    951.0,
    270.0,
    image=image_image_2
)

canvas.create_text(
    863.0,
    9.0,
    anchor="nw",
    text="100%",
    fill="#282828",
    font=("IBM Plex Mono SemiBold", 32 * -1)
)

window.bind('<Left>', move_images)
window.bind('<Right>', move_images)


window.bind('<Return>', open_method) # Use <Return> for Enter key ti

window.resizable(False, False)
window.mainloop()
