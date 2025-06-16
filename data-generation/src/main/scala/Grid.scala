import scala.util.Random
import play.api.libs.json._
import JsonFormats._

sealed trait CellType extends Serializable
case object Water extends CellType
case object Grass extends CellType
case object Tree extends CellType
case object Sapling extends CellType
case object YoungTree extends CellType
case object BurningTree1 extends CellType
case object BurningTree2 extends CellType
case object BurningTree3 extends CellType
case object BurningSapling extends CellType
case object BurningYoungTree1 extends CellType
case object BurningYoungTree2 extends CellType
case object BurningGrass extends CellType
case object Thunder extends CellType
case class Ash(deadSteps: Int) extends CellType
case class BurnedGrass(deadSteps: Int) extends CellType
case class Cell(cellType: CellType, growSteps: Int = 0)

object JsonFormats {
  implicit val cellTypeWrites: Writes[CellType] = Writes {
    case Water             => JsString("W")
    case Grass             => JsString("G")
    case Tree              => JsString("T")
    case Sapling           => JsString("s")
    case YoungTree         => JsString("y")
    case BurningTree1      => JsString("*")
    case BurningTree2      => JsString("**")
    case BurningTree3      => JsString("***")
    case BurningSapling    => JsString("!")
    case BurningYoungTree1 => JsString("&")
    case BurningYoungTree2 => JsString("@")
    case BurningGrass      => JsString("+")
    case Thunder           => JsString("TH")
    case Ash(_)            => JsString("A")
    case BurnedGrass(_)    => JsString("-")
  }
  implicit val vectorStringWrites: Writes[Vector[String]] = Writes { vs =>
    JsArray(vs.map(JsString(_)))
  }
  implicit val vectorVectorStringWrites: Writes[Vector[Vector[String]]] =
    Writes { vvs => JsArray(vvs.map(v => Json.toJson(v))) }
}

case class Grid(
    width: Int,
    height: Int,
    cells: Vector[Vector[Cell]],
    rand: Random,

    // --- REGROWTH TIMING PARAMETERS (in days) ---
    ashRegrowSteps: Int =
      300, // Days before regrowth can start on ash (≈ 10 months)
    burnedGrassRegrowSteps: Int =
      15, // Days before burned grass can recover (≈ 2 weeks)
    saplingGrowSteps: Int =
      60, // Days for sapling to become a young tree (≈ 2 months)
    youngTreeGrowSteps: Int =
      180, // Days for young tree to become mature (≈ 6 months)

    // --- IGNITION PROBABILITIES (per burning neighbor, per day) ---
    treeIgniteProb: Double =
      0.02, // Probability a tree ignites from a burning neighbor each day (2%)
    grassIgniteProb: Double =
      0.08, // Probability grass ignites from a burning neighbor each day (8%)

    // --- WIND EFFECT PARAMETERS ---
    windSteepness: Double =
      0.4, // Controls sharpness of wind effect (higher = more abrupt transition)
    windMidpoint: Double =
      20.0, // Wind speed (km/h) at which fire spread sharply increases ("critical wind")
    windMaxMult: Double =
      7.0, // Maximum fire spread multiplier at highest wind (e.g. 7× at 50 km/h)

    // --- FIRE JUMP ("SPOTTING") PARAMETERS ---
    fireJumpBaseChance: Double =
      0.002, // Base daily probability of fire "jumping" (spotting) to distant cells (0.2%)
    fireJumpDistFactor: Double =
      3.0, // Distance divisor for spotting chance (higher = less likely at distance)
    fireJumpMaxMult: Double =
      5.0, // Max wind multiplier for spotting chance (should match windMaxMult)

    // --- POST-FIRE REGENERATION PROBABILITIES (per day) ---
    ashToTreeProb: Double =
      0.03, // Probability ash regrows as sapling (per day, after regrow delay)
    burnedGrassToGrassProb: Double =
      0.4 // Probability burned grass regrows as grass (per day, after regrow delay)
) {
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

  // Classic sigmoid function
  def sigmoid(x: Double): Double = 1.0 / (1.0 + math.exp(-x))

  // Wind effect: 1 + (maxMult-1) * sigmoid
  def windAmplifier(
      windStrength: Int,
      steepness: Double,
      midpoint: Double,
      maxMult: Double
  ): Double = {
    val sig = sigmoid(steepness * (windStrength - midpoint))
    1.0 + (maxMult - 1.0) * sig
  }

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

  // Wind-driven fire jump with sigmoid wind effect
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

  def strikeThunder(percentage: Int): Grid = {
    // 1. Collect all tree positions
    val treePositions = for {
      y <- 0 until height
      x <- 0 until width
      if cells(y)(x).cellType == Tree
    } yield (x, y)

    // 2. How many to strike?
    val numToStrike = math.ceil(treePositions.size * percentage / 100.0).toInt
    val toStrike: Set[(Int, Int)] =
      rand.shuffle(treePositions).take(numToStrike).toSet

    // 3. Replace those with Thunder
    val newCells = cells.zipWithIndex.map { case (row, y) =>
      row.zipWithIndex.map { case (cell, x) =>
        if (toStrike.contains((x, y))) Cell(Thunder)
        else cell
      }
    }
    this.copy(cells = newCells)
  }

  def getCell(x: Int, y: Int): Option[Cell] =
    if (x >= 0 && x < width && y >= 0 && y < height) Some(cells(y)(x)) else None

  def isBurning(cellType: CellType): Boolean = cellType match {
    case BurningTree1 | BurningTree2 | BurningTree3 | BurningGrass |
        BurningSapling | BurningYoungTree1 | BurningYoungTree2 =>
      true
    case _ => false
  }

  def isLivingOrWater(cellType: CellType): Boolean = cellType match {
    case Water | Grass | Tree | Sapling | YoungTree => true
    case _                                          => false
  }

  def hasLivingOrWaterNeighbor(x: Int, y: Int): Boolean =
    neighborDirs.exists { case (dx, dy) =>
      getCell(x + dx, y + dy).exists(c => isLivingOrWater(c.cellType))
    }

  def encodeCells: Vector[Vector[String]] =
    cells.map(
      _.map(cell => JsonFormats.cellTypeWrites.writes(cell.cellType).as[String])
    )

  def nextStep(
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

        case Sapling =>
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
          if (ignites || jump) Cell(BurningSapling)
          else if (grow >= saplingGrowSteps) Cell(YoungTree)
          else Cell(Sapling, grow + 1)

        case YoungTree =>
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
          if (ignites || jump) Cell(BurningYoungTree1)
          else if (grow >= youngTreeGrowSteps) Cell(Tree)
          else Cell(YoungTree, grow + 1)

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

        case Thunder           => Cell(BurningTree1)
        case BurningTree1      => Cell(BurningTree2)
        case BurningTree2      => Cell(BurningTree3)
        case BurningTree3      => Cell(Ash(0))
        case BurningGrass      => Cell(BurnedGrass(0))
        case BurningSapling    => Cell(Ash(0))
        case BurningYoungTree1 => Cell(BurningYoungTree2)
        case BurningYoungTree2 => Cell(Ash(0))

        case Ash(deadSteps) =>
          if (
            deadSteps >= ashRegrowSteps - 1 && hasLivingOrWaterNeighbor(x, y)
          ) {
            if (rand.nextDouble() < ashToTreeProb) Cell(Sapling)
            else Cell(Grass)
          } else Cell(Ash(deadSteps + 1))

        case BurnedGrass(deadSteps) =>
          if (
            deadSteps >= burnedGrassRegrowSteps - 1 && hasLivingOrWaterNeighbor(
              x,
              y
            )
          ) {
            if (rand.nextDouble() < burnedGrassToGrassProb) Cell(Grass)
            else Cell(Sapling)
          } else Cell(BurnedGrass(deadSteps + 1))

        case _ => cells(y)(x)
      }
    }
    val updatedGrid = this.copy(cells = newCells, rand = this.rand)
    if (doThunderThisStep)
      updatedGrid.strikeThunder(thunderPercentage)
    else
      updatedGrid
  }
}

object Grid {
  // Factory for initial grid, as needed by Main.scala!
  def randomCell(rand: Random): Cell = rand.nextInt(100) match {
    case n if n < 5  => Cell(Water)
    case n if n < 25 => Cell(Grass)
    case _           => Cell(Tree)
  }
  def apply(width: Int, height: Int, rand: Random): Grid = {
    val cells = Vector.tabulate(height, width)((_, _) => randomCell(rand))
    Grid(width, height, cells, rand)
  }
}
