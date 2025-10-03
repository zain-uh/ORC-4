import tkinter as tk
import subprocess

#RUN APP WITH TERMINAL COMMAND python pi_panel.py
#Use arrow keys to navigate
#EVERYTHING WILL BE PURGED

# Apps list: (label, command)
apps = [
    ("YouTube", "chromium-browser --kiosk https://youtube.com"),
    ("Files", "pcmanfm")
]

# Defining variables and functions
current_index = 0  # start at first app

def launch_app():
    label, cmd = apps[current_index]
    subprocess.Popen(cmd, shell=True)

def show_panel():
    global current_index
    app_label.config(text=apps[current_index][0])

def next_panel(event=None):
    global current_index
    current_index = (current_index + 1) % len(apps)
    show_panel()

def prev_panel(event=None):
    global current_index
    current_index = (current_index - 1) % len(apps)
    show_panel()

# Main window
root = tk.Tk()
root.title("Pi Panels")
root.attributes("-fullscreen", False)  #Set to False for testing

# Label for app name
app_label = tk.Label(root, text="", font=("Arial", 60))
app_label.pack(expand=True)

# Launch button
launch_btn = tk.Button(root, text="Launch", font=("Arial", 40), command=launch_app)
launch_btn.pack(pady=40)

# Key bindings
root.bind("<Right>", next_panel)
root.bind("<Left>", prev_panel)
root.bind("<Return>", lambda e: launch_app())

# Show first app
show_panel()

root.mainloop()