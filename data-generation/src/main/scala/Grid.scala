import scala.util.Random
import play.api.libs.json._
import JsonFormats._

/** All possible cell types for the forest fire simulation. Each cell serializes
  * to a specific NDJSON short code (noted for each).
  */
sealed trait CellType extends Serializable

// ─────────────── Basic Terrain and Vegetation ───────────────

/** "W" — Water tile, never burns or regrows. */
case object Water extends CellType

/** "G" — Grass, burns quickly, regrows quickly. */
case object Grass extends CellType

/** "T" — Mature tree, fully grown, main slow-burning vegetation. */
case object Tree extends CellType

/** "s" — Sapling, the youngest tree stage. Grows into a young tree. */
case object GrowingTree1 extends CellType

/** "y" — Young tree. Second stage after sapling. Grows into mature tree. */
case object GrowingTree2 extends CellType

// ─────────────── Burning Stages ───────────────

/** "*"   — Burning mature tree, first burning stage. */
case object BurningTree1 extends CellType

/** "**"  — Burning mature tree, second burning stage. */
case object BurningTree2 extends CellType

/** "***" — Burning mature tree, third and last burning stage. */
case object BurningTree3 extends CellType

/** "+"   — Burning grass, burns out quickly. */
case object BurningGrass extends CellType

/** "!"   — Burning sapling (GrowingTree1 caught fire). */
case object BurningGrowingTree1 extends CellType

/** "&"   — Burning young tree, first stage. */
case object BurningGrowingTree2_1 extends CellType

/** "@"   — Burning young tree, second (final) stage. */
case object BurningGrowingTree2_2 extends CellType

// ─────────────── After Fire ───────────────

/** "A" — Ash from burned trees, may regrow over time. */
case object BurnedTree extends CellType

/** "-" — Ash from burned grass, regrows quickly. */
case object BurnedGrass extends CellType

// ─────────────── Special Event ───────────────

/** "TH" — Thunder strike, causes tree at that location to catch fire. */
case object Thunder extends CellType

// ─────────────── Step-tracking Types (for internal use) ───────────────

/** Internal: tracks steps since burning for trees, outputs as "A". */
case class Ash(deadSteps: Int) extends CellType

/** Internal: tracks steps since burning for grass, outputs as "-". */
case class BurnedGrassSteps(deadSteps: Int) extends CellType

/** Represents a single cell in the grid, with type and optional growth counter.
  * growSteps is used for GrowingTree1 and GrowingTree2 to track regrowth.
  */
case class Cell(cellType: CellType, growSteps: Int = 0)

// ────────────────────── Serialization Formats ──────────────────────

object JsonFormats {
  // Serializes each cell type as a single-character NDJSON code.
  implicit val cellTypeWrites: Writes[CellType] = Writes {
    case Water                 => JsString("W")
    case Grass                 => JsString("G")
    case Tree                  => JsString("T")
    case GrowingTree1          => JsString("s")
    case GrowingTree2          => JsString("y")
    case BurningTree1          => JsString("*")
    case BurningTree2          => JsString("**")
    case BurningTree3          => JsString("***")
    case BurningGrass          => JsString("+")
    case BurningGrowingTree1   => JsString("!")
    case BurningGrowingTree2_1 => JsString("&")
    case BurningGrowingTree2_2 => JsString("@")
    case BurnedTree            => JsString("A")
    case BurnedGrass           => JsString("-")
    case Thunder               => JsString("TH")
    case Ash(_)                => JsString("A")
    case BurnedGrassSteps(_)   => JsString("-")
  }
  implicit val vectorStringWrites: Writes[Vector[String]] = Writes { vs =>
    JsArray(vs.map(JsString(_)))
  }
  implicit val vectorVectorStringWrites: Writes[Vector[Vector[String]]] =
    Writes { vvs => JsArray(vvs.map(v => Json.toJson(v))) }
}

// ────────────────────── Main Simulation Grid ──────────────────────

case class Grid(
    width: Int,
    height: Int,
    cells: Vector[Vector[Cell]],
    rand: Random,

    // --- Regrowth timing (in steps = days) ---
    ashRegrowSteps: Int =
      300, // Days before regrowth can start on ash (≈10 months)
    burnedGrassRegrowSteps: Int =
      15, // Days before burned grass can recover (≈2 weeks)
    saplingGrowSteps: Int = 60, // Days for sapling → young tree (≈2 months)
    youngTreeGrowSteps: Int =
      180, // Days for young tree → mature tree (≈6 months)

    // --- Ignition probabilities (per burning neighbor, per day) ---
    treeIgniteProb: Double =
      0.02, // Probability a tree ignites per burning neighbor/day
    grassIgniteProb: Double =
      0.08, // Probability grass ignites per burning neighbor/day

    // --- Wind effect parameters ---
    windSteepness: Double = 0.4, // Controls sharpness of wind effect
    windMidpoint: Double =
      20.0, // Wind speed (km/h) where fire spread accelerates
    windMaxMult: Double = 7.0, // Max fire spread multiplier at high wind

    // --- Fire jump ("spotting") parameters ---
    fireJumpBaseChance: Double =
      0.002, // Base chance fire "jumps" ahead per day (0.2%)
    fireJumpDistFactor: Double = 3.0, // Distance divisor for jump chance
    fireJumpMaxMult: Double = 5.0, // Max wind multiplier for spotting

    // --- Post-fire regeneration probabilities (per day, after regrow delay) ---
    ashToTreeProb: Double = 0.03, // Chance that ash regrows as sapling (3%/day)
    burnedGrassToGrassProb: Double =
      0.4 // Chance that burned grass regrows as grass (40%/day)
) {
  // Relative neighbor coordinates for 8 directions.
  private val neighborDirs = List(
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1)
  )

  // Sigmoid function (used for wind/fire amplification)
  def sigmoid(x: Double): Double = 1.0 / (1.0 + math.exp(-x))

  /** Computes wind amplification multiplier for fire spread. */
  def windAmplifier(
      windStrength: Int,
      steepness: Double,
      midpoint: Double,
      maxMult: Double
  ): Double = {
    val sig = sigmoid(steepness * (windStrength - midpoint))
    1.0 + (maxMult - 1.0) * sig
  }

  /** Computes wind-adjusted ignition probability in the direction (dx, dy). */
  def windAdjustedProb(
      baseProb: Double,
      dx: Int,
      dy: Int,
      windVec: (Double, Double),
      windStrength: Int
  ): Double = {
    val nrm = math.sqrt(dx * dx + dy * dy)
    if (nrm == 0) baseProb
    else {
      val dir = (dx / nrm, dy / nrm)
      val alignment = windVec._1 * dir._1 + windVec._2 * dir._2
      val mult = windAmplifier(
        windStrength,
        windSteepness,
        windMidpoint,
        windMaxMult
      )
      baseProb * (1.0 + alignment) * mult
    }
  }

  /** Determines if fire can "jump" ahead due to wind (spotting ignition). */
  def fireJumped(
      x: Int,
      y: Int,
      windAngle: Int,
      windStrength: Int
  ): Boolean = {
    val windRad = math.toRadians(windAngle)
    val jumpDistances = List(2, 3, 4)
    val jumpMult = windAmplifier(
      windStrength,
      windSteepness,
      windMidpoint + 2.0,
      fireJumpMaxMult
    )
    jumpDistances.exists { dist =>
      val dx = Math.round(Math.sin(windRad) * dist).toInt
      val dy = Math.round(-Math.cos(windRad) * dist).toInt
      getCell(x + dx, y + dy).exists {
        case Cell(burnCt, _) if isBurning(burnCt) =>
          val jumpProb =
            fireJumpBaseChance * jumpMult / (dist * fireJumpDistFactor)
          rand.nextDouble() < jumpProb
        case _ => false
      }
    }
  }

  /** Randomly ignites a percentage of trees and grasses at simulation start. */
  def igniteRandomFires(percentTrees: Double, percentGrass: Double): Grid = {
    val treePositions = for {
      y <- 0 until height
      x <- 0 until width
      if cells(y)(x).cellType == Tree
    } yield (x, y)
    val grassPositions = for {
      y <- 0 until height
      x <- 0 until width
      if cells(y)(x).cellType == Grass
    } yield (x, y)
    def pickN[T](xs: Seq[T], n: Int): Set[T] = rand.shuffle(xs).take(n).toSet

    val selectedTrees =
      pickN(treePositions, (treePositions.size * percentTrees / 100).toInt)
    val selectedGrass =
      pickN(grassPositions, (grassPositions.size * percentGrass / 100).toInt)

    val newCells = cells.zipWithIndex.map { case (row, y) =>
      row.zipWithIndex.map { case (cell, x) =>
        if (selectedTrees.contains((x, y))) Cell(BurningTree1)
        else if (selectedGrass.contains((x, y))) Cell(BurningGrass)
        else cell
      }
    }
    this.copy(cells = newCells, rand = this.rand)
  }

  /** Strikes thunder on random trees, turning them into Thunder cells. */
  def strikeThunder(percentage: Int): Grid = {
    val treePositions = for {
      y <- 0 until height
      x <- 0 until width
      if cells(y)(x).cellType == Tree
    } yield (x, y)

    val numToStrike = math.ceil(treePositions.size * percentage / 100.0).toInt
    val toStrike: Set[(Int, Int)] =
      rand.shuffle(treePositions).take(numToStrike).toSet

    val newCells = cells.zipWithIndex.map { case (row, y) =>
      row.zipWithIndex.map { case (cell, x) =>
        if (toStrike.contains((x, y))) Cell(Thunder)
        else cell
      }
    }
    this.copy(cells = newCells)
  }

  /** Returns the cell at (x, y), or None if out of bounds. */
  def getCell(x: Int, y: Int): Option[Cell] =
    if (x >= 0 && x < width && y >= 0 && y < height) Some(cells(y)(x)) else None

  /** Returns true if the cell type is burning. */
  def isBurning(cellType: CellType): Boolean = cellType match {
    case BurningTree1 | BurningTree2 | BurningTree3 | BurningGrass |
        BurningGrowingTree1 | BurningGrowingTree2_1 | BurningGrowingTree2_2 =>
      true
    case _ => false
  }

  /** Returns true if the cell type is living vegetation or water. */
  def isLivingOrWater(cellType: CellType): Boolean = cellType match {
    case Water | Grass | Tree | GrowingTree1 | GrowingTree2 => true
    case _                                                  => false
  }

  /** Returns true if (x, y) has any living or water neighbor. */
  def hasLivingOrWaterNeighbor(x: Int, y: Int): Boolean =
    neighborDirs.exists { case (dx, dy) =>
      getCell(x + dx, y + dy).exists(c => isLivingOrWater(c.cellType))
    }

  /** Serializes the grid as NDJSON codes (for output). */
  def encodeCells: Vector[Vector[String]] =
    cells.map(
      _.map(cell => JsonFormats.cellTypeWrites.writes(cell.cellType).as[String])
    )

  /** Advances the grid by one step, applying fire, regrowth, thunder, and wind.
    */
  def nextStep(
      enableThunder: Boolean,
      thunderPercentage: Int,
      enableWind: Boolean,
      windAngle: Int,
      windStrength: Int,
      doThunderThisStep: Boolean
  ): Grid = {
    val windRad = math.toRadians(windAngle)
    val windVec = (math.sin(windRad), -math.cos(windRad))
    val newCells = Vector.tabulate(height, width) { (y, x) =>
      val Cell(ct, grow) = cells(y)(x)
      ct match {
        // Mature tree: burns from burning neighbor or jump
        case Tree =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy).exists {
              case Cell(burnCt, _) if isBurning(burnCt) =>
                rand.nextDouble() < (
                  if (enableWind)
                    windAdjustedProb(
                      treeIgniteProb,
                      dx,
                      dy,
                      windVec,
                      windStrength
                    )
                  else treeIgniteProb
                )
              case _ => false
            }
          }
          val jump =
            enableWind && !ignites && fireJumped(x, y, windAngle, windStrength)
          if (ignites || jump) Cell(BurningTree1) else Cell(Tree)

        // Sapling: burns, or grows to young tree
        case GrowingTree1 =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy).exists {
              case Cell(burnCt, _) if isBurning(burnCt) =>
                rand.nextDouble() < (
                  if (enableWind)
                    windAdjustedProb(
                      treeIgniteProb * 1.2,
                      dx,
                      dy,
                      windVec,
                      windStrength
                    )
                  else treeIgniteProb * 1.2
                )
              case _ => false
            }
          }
          val jump =
            enableWind && !ignites && fireJumped(x, y, windAngle, windStrength)
          if (ignites || jump) Cell(BurningGrowingTree1)
          else if (grow >= saplingGrowSteps) Cell(GrowingTree2)
          else Cell(GrowingTree1, grow + 1)

        // Young tree: burns, or grows to mature tree
        case GrowingTree2 =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy).exists {
              case Cell(burnCt, _) if isBurning(burnCt) =>
                rand.nextDouble() < (
                  if (enableWind)
                    windAdjustedProb(
                      treeIgniteProb * 1.1,
                      dx,
                      dy,
                      windVec,
                      windStrength
                    )
                  else treeIgniteProb * 1.1
                )
              case _ => false
            }
          }
          val jump =
            enableWind && !ignites && fireJumped(x, y, windAngle, windStrength)
          if (ignites || jump) Cell(BurningGrowingTree2_1)
          else if (grow >= youngTreeGrowSteps) Cell(Tree)
          else Cell(GrowingTree2, grow + 1)

        // Grass: burns from neighbor, regrows quickly
        case Grass =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy).exists {
              case Cell(burnCt, _) if isBurning(burnCt) =>
                rand.nextDouble() < (
                  if (enableWind)
                    windAdjustedProb(
                      grassIgniteProb,
                      dx,
                      dy,
                      windVec,
                      windStrength
                    )
                  else grassIgniteProb
                )
              case _ => false
            }
          }
          val jump =
            enableWind && !ignites && fireJumped(x, y, windAngle, windStrength)
          if (ignites || jump) Cell(BurningGrass) else Cell(Grass)

        // Thunder strikes: instantly turns tree into burning
        case Thunder               => Cell(BurningTree1)
        case BurningTree1          => Cell(BurningTree2)
        case BurningTree2          => Cell(BurningTree3)
        case BurningTree3          => Cell(BurnedTree)
        case BurningGrass          => Cell(BurnedGrass)
        case BurningGrowingTree1   => Cell(BurnedTree)
        case BurningGrowingTree2_1 => Cell(BurningGrowingTree2_2)
        case BurningGrowingTree2_2 => Cell(BurnedTree)

        // Ash and burned grass regrow after enough time and with a neighbor
        case Ash(deadSteps) =>
          if (
            deadSteps >= ashRegrowSteps - 1 && hasLivingOrWaterNeighbor(x, y)
          ) {
            if (rand.nextDouble() < ashToTreeProb) Cell(GrowingTree1)
            else Cell(Grass)
          } else Cell(Ash(deadSteps + 1))

        case BurnedGrassSteps(deadSteps) =>
          if (
            deadSteps >= burnedGrassRegrowSteps - 1 && hasLivingOrWaterNeighbor(
              x,
              y
            )
          ) {
            if (rand.nextDouble() < burnedGrassToGrassProb) Cell(Grass)
            else Cell(GrowingTree1)
          } else Cell(BurnedGrassSteps(deadSteps + 1))

        case BurnedTree  => Cell(BurnedTree)
        case BurnedGrass => Cell(BurnedGrass)

        // Water and unknowns: remain unchanged
        case _ => cells(y)(x)
      }
    }
    val updatedGrid = this.copy(cells = newCells, rand = this.rand)
    if (doThunderThisStep && enableThunder)
      updatedGrid.strikeThunder(thunderPercentage)
    else
      updatedGrid
  }
}

object Grid {
  // Specify Tree and Grass percentages (Water gets the rest)
  val InitialTreePercent = 70
  val InitialGrassPercent = 20
  val TotalPercent = 100

  /** Factory for random initial grid. */
  def randomCell(rand: Random): Cell = {
    val roll = rand.nextInt(TotalPercent)
    if (roll < InitialTreePercent)
      Cell(Tree)
    else if (roll < InitialTreePercent + InitialGrassPercent)
      Cell(Grass)
    else
      Cell(Water)
  }

  def apply(width: Int, height: Int, rand: Random): Grid = {
    val cells = Vector.tabulate(height, width)((_, _) => randomCell(rand))
    Grid(width, height, cells, rand)
  }
}
