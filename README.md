# DIY Stereo Volume Control Box

In this project, I am documenting a beginner experience creating a "DIY" stereo
volume control box. Since audio signal processing seemed to be a hard thing to
get right as a total newbie, I decided to go with a prebuilt circuit that would
alter the volume of an analoge signal and focused on controlling the volume
remotely.

## Parts used

- [Digitally operated Stereo Volume circuit](https://de.aliexpress.com/item/1005008418754756.html)
- ESP32

## Controlling the Wondom AA-AB41148 Volume Control Board using the GPIO Pins

After buying the Wondom Board I found out that while there are GPIO pins
available, the correct usage of those pins is not documented anywhere and seems
to be actively discouraged by the manufacturer:

> It's not suggested to control this board by your own because you cannot
> control this PGA2311 volume board with separate SPI or I2C.

So, next time I'll actually read the full datasheet of a product before sinking
30€ into it. Nevertheless, I still wanted to try getting this to work so I did
some experiments.

> [!WARNING]
> I didn't really have any experiments with electronic circuits when I tried to
> understand how this board is controlled, so don't take this as a tutorial but
> more as a tale of exploration. Sorry for not distilling this down to the most
> important parts, since I cannot make those out yet.\
> If you have any advice on what I could have done better, feel free to reach
> out the me / create an issue.

### Understanding / "Reverse engineering" the decoder board

Since the board came with a AA-AB41152 potentiometer board, I decided the
general idea would be to understand which signals that board sends to the volume
board and try to replicate that using the ESP32.

There are both a 4-pin-connector to the potentiometer board and 4 GPIO pins that
have the same labels with `5V`, `GND`, `DATA` and `CLOCK`. Using a multimeter, I
first made sure that the `DATA` and `CLOCK` lines were connected between the
GPIO and 4-pin-connector, which will be useful to read the signal from the ESP32
while the potentiometer board is sending signals to the volume board.

With the decoder board connected, both the `DATA` and `CLOCK` were idling around
0V. I expected both of the lines to go up to 5V while transmitting data. Since
connecting to 5V is not save for the ESP32, I hooked up two GPIO PINs of the
ESP32 to `DATA` and `CLOCK` using voltage dividers to bring the voltage down
less then ~3.5V (2.5V in my case, since I only had 10K resistors around).

For sniffing out the data send over the decoder board, I wrote a short program
under `bin/potentiometer_sniffers.rs` that logs individual segments. Here are
some examples:

Volume up:

```
--- Frame (16 bits) ---
0101110001011100
HEX: 5C 5C
-----------------------
--- Frame (16 bits) ---
0101111001011110
HEX: 5E 5E
-----------------------
--- Frame (16 bits) ---
0110000001100000
HEX: 60 60
-----------------------
```

Volume down:

```
--- Frame (16 bits) ---
0111001001110010
HEX: 72 72
-----------------------
--- Frame (16 bits) ---
0111000001110000
HEX: 70 70
-----------------------
--- Frame (16 bits) ---
0110111001101110
HEX: 6E 6E
-----------------------
```

And then we have some bigger segments when pressing the knob (muting):

```
// Muting / Unmuting
--- Frame (128 bits) ---
01110100011101000111001001110010011100000111000001101110011011100110110001101100011010100110101001101000011010000110011001100110
HEX: 74 74 72 72 70 70 6E 6E 6C 6C 6A 6A 68 68 66 66
-----------------------
--- Frame (128 bits) ---
00000000000000000001010000010100001010000010100001001100010011000100111001001110010100000101000001010010010100100101010001010100
HEX: 00 00 14 14 28 28 4C 4C 4E 4E 50 50 52 52 54 54
-----------------------
```

There were also some segments send when turning on the board, but these won't be
emulated by the ESP32 so I didn't bother with that.

It looks like turning the knob one step at a time just increments the volume by
2 and sends the new volume twice as 8 bit values. When muting or unmuting, a
similar pattern can be observed with a rapid increase / decrease in volume.

### Emulating the decoder

The first thing to think about is how we should send data from the ESP32 to the
volume board safely while the decoder board is attached. I wanted to keep the
knob so I could still do manual adjustments in case something with the ESP32 is
not working correctly.
