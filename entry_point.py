import os
import sys
from pathlib import Path

if getattr(sys, 'frozen', False):
    BASE_DIR = Path(sys._MEIPASS).resolve()
else:
    BASE_DIR = Path(__file__).parent.resolve()

os.chdir(BASE_DIR)

from src.app import CSCECPaymentApp

if __name__ == "__main__":
    app = CSCECPaymentApp()
    app.mainloop()
